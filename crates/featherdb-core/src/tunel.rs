//! Túnel SSH: o banco fica atrás de um servidor que só abre a porta 22. O FeatherDB abre uma porta
//! local (127.0.0.1, escolhida pelo sistema) e cada conexão nela vira um canal SSH até o banco.
//! O túnel vive enquanto a conexão do banco viver (`ComTunel`).
use std::sync::{Arc, Mutex};

use russh::client;
use russh::keys::{self, PrivateKeyWithHashAlg, PublicKeyOrCertificate};

use crate::{Banco, CampoDe, Cancelar, Coluna, Conexao, Funcao, Resultado, Tabela};

/// Confere a chave do servidor SSH no `~/.ssh/known_hosts`: servidor novo é anotado (como o
/// `StrictHostKeyChecking=accept-new` do ssh); chave diferente da anotada é recusada.
struct Confere {
    host: String,
    porta: u16,
    erro: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Confere {
    type Error = russh::Error;

    async fn check_server_key(&mut self, chave: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let chave = match chave {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(c) => c.public_key().clone().into(),
        };
        match keys::check_known_hosts(&self.host, self.porta, &chave) {
            Ok(true) => Ok(true),
            Ok(false) => {
                let _ = keys::known_hosts::learn_known_hosts(&self.host, self.porta, &chave);
                Ok(true)
            }
            Err(keys::Error::KeyChanged { line }) => {
                *self.erro.lock().unwrap() = Some(format!(
                    "A chave do servidor SSH {} mudou (linha {line} do ~/.ssh/known_hosts). Pode ser outro servidor no lugar dele: confira com quem cuida do servidor antes de apagar a linha.",
                    self.host
                ));
                Ok(false)
            }
            // Sem known_hosts legível (pasta ~/.ssh ausente, por exemplo): aceita, como na primeira vez.
            Err(_) => Ok(true),
        }
    }
}

pub(crate) struct Tunel {
    pub(crate) porta: u16,
    // O túnel roda neste tokio; ele para quando o túnel é largado.
    _rt: tokio::runtime::Runtime,
}

pub(crate) fn abrir(c: &Conexao, destino: (String, u16)) -> Result<Tunel, String> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().map_err(|e| e.to_string())?;
    let porta = rt.block_on(conectar(c, destino))?;
    Ok(Tunel { porta, _rt: rt })
}

async fn conectar(c: &Conexao, (host_banco, porta_banco): (String, u16)) -> Result<u16, String> {
    let erro = Arc::new(Mutex::new(None));
    let confere = Confere { host: c.ssh_host.trim().to_owned(), porta: c.ssh_porta, erro: erro.clone() };
    let config = Arc::new(client::Config { inactivity_timeout: None, keepalive_interval: Some(std::time::Duration::from_secs(30)), ..Default::default() });
    let endereco = (c.ssh_host.trim().to_owned(), c.ssh_porta);
    let conectando = client::connect(config, endereco, confere);
    let mut sessao = match tokio::time::timeout(std::time::Duration::from_secs(15), conectando).await {
        Err(_) => return Err(format!("O servidor SSH {} não respondeu em 15 s.", c.ssh_host)),
        Ok(Err(e)) => return Err(erro.lock().unwrap().take().unwrap_or_else(|| format!("SSH: {e}"))),
        Ok(Ok(s)) => s,
    };
    let usuario = c.ssh_usuario.trim().to_owned();
    let ok = if c.ssh_chave.trim().is_empty() {
        sessao.authenticate_password(usuario, c.ssh_senha.clone()).await.map_err(|e| format!("SSH: {e}"))?.success()
    } else {
        // Com chave, a senha do SSH (se houver) é a da chave.
        let frase = (!c.ssh_senha.is_empty()).then_some(c.ssh_senha.as_str());
        let chave = keys::load_secret_key(c.ssh_chave.trim(), frase).map_err(|e| format!("Chave SSH ({}): {e}", c.ssh_chave))?;
        let hash = sessao.best_supported_rsa_hash().await.map_err(|e| format!("SSH: {e}"))?.flatten();
        sessao.authenticate_publickey(usuario, PrivateKeyWithHashAlg::new(Arc::new(chave), hash)).await.map_err(|e| format!("SSH: {e}"))?.success()
    };
    if !ok {
        return Err("O servidor SSH recusou o usuário, a senha ou a chave.".into());
    }
    let ouvinte = tokio::net::TcpListener::bind("127.0.0.1:0").await.map_err(|e| e.to_string())?;
    let porta = ouvinte.local_addr().map_err(|e| e.to_string())?.port();
    let sessao = Arc::new(sessao);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = ouvinte.accept().await {
            let (sessao, host) = (sessao.clone(), host_banco.clone());
            tokio::spawn(async move {
                if let Ok(canal) = sessao.channel_open_direct_tcpip(host, u32::from(porta_banco), "127.0.0.1", u32::from(porta)).await {
                    let mut fluxo = canal.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut fluxo).await;
                }
            });
        }
    });
    Ok(porta)
}

#[cfg(test)]
mod testes {
    #[test]
    fn servidor_ssh_que_nao_atende_da_erro_claro() {
        // Porta 1 do próprio computador: ninguém escuta, a conexão é recusada na hora.
        let c = crate::Conexao { ssh_host: "127.0.0.1".into(), ssh_porta: 1, ssh_usuario: "x".into(), ..crate::Conexao::nova(crate::Tipo::Postgres) };
        let e = super::abrir(&c, ("db".into(), 5432)).err().unwrap();
        assert!(e.starts_with("SSH:"), "{e}");
    }
}

/// A conexão do banco com o túnel junto: largar a conexão fecha o túnel.
pub(crate) struct ComTunel {
    pub(crate) banco: Box<dyn Banco>,
    pub(crate) _tunel: Tunel,
}

impl Banco for ComTunel {
    fn cancelador(&self) -> Option<Cancelar> {
        self.banco.cancelador()
    }
    fn executar(&mut self, sql: &str, limite: usize) -> Result<Resultado, String> {
        self.banco.executar(sql, limite)
    }
    fn tabelas(&mut self) -> Result<Vec<Tabela>, String> {
        self.banco.tabelas()
    }
    fn bancos(&mut self) -> Result<Vec<String>, String> {
        self.banco.bancos()
    }
    fn funcoes(&mut self) -> Result<Vec<Funcao>, String> {
        self.banco.funcoes()
    }
    fn definicao(&mut self, f: &Funcao) -> Result<String, String> {
        self.banco.definicao(f)
    }
    fn colunas(&mut self, t: &Tabela) -> Result<Vec<Coluna>, String> {
        self.banco.colunas(t)
    }
    fn ddl(&mut self, t: &Tabela) -> Result<String, String> {
        self.banco.ddl(t)
    }
    fn dicionario(&mut self) -> Result<Vec<CampoDe>, String> {
        self.banco.dicionario()
    }
}

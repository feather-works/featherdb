//! As senhas lembradas ficam no cofre de senhas do sistema (GNOME Keyring/KWallet pelo Secret
//! Service no Linux, Gerenciador de Credenciais no Windows, Keychain no macOS), nunca num arquivo.
use crate::Conexao;

const SERVICO: &str = "FeatherDB";

/// A chave da senha no cofre: tipo, usuário, servidor e banco (ou o arquivo/string de conexão).
fn conta(c: &Conexao) -> String {
    match c.tipo {
        crate::Tipo::Postgres | crate::Tipo::MySql => format!("{:?}://{}@{}:{}/{}", c.tipo, c.usuario, c.host, c.porta, c.banco),
        _ => format!("{:?}://{}@{}", c.tipo, c.usuario, c.caminho),
    }
}

fn entrada(c: &Conexao) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICO, &conta(c)).map_err(|e| e.to_string())
}

/// A senha guardada, se houver.
/// shortcut: chamado na thread da interface; se o cofre estiver trancado o sistema pede para destrancar
/// e a janela espera. Passar para segundo plano se isso incomodar.
pub fn ler(c: &Conexao) -> Option<String> {
    entrada(c).ok()?.get_password().ok()
}

pub fn gravar(c: &Conexao, senha: &str) -> Result<(), String> {
    entrada(c)?.set_password(senha).map_err(|e| e.to_string())
}

/// Apaga a senha guardada (sem senha guardada não é erro).
pub fn apagar(c: &Conexao) {
    if let Ok(e) = entrada(c) {
        let _ = e.delete_credential();
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Usa o cofre de verdade da máquina: `cargo test -p featherdb-core -- --ignored cofre`.
    #[test]
    #[ignore]
    fn cofre_ida_e_volta() {
        let c = Conexao { host: "featherdb-teste.invalid".into(), usuario: "teste".into(), ..Conexao::nova(crate::Tipo::Postgres) };
        gravar(&c, "s3nh@ção").unwrap();
        assert_eq!(ler(&c).as_deref(), Some("s3nh@ção"));
        apagar(&c);
        assert_eq!(ler(&c), None);
    }
}

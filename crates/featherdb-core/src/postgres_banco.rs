//! PostgreSQL pelo protocolo simples: o servidor devolve todo valor já como texto, de qualquer tipo.
use std::sync::Arc;

use postgres::{Client, NoTls, SimpleQueryMessage};

use crate::{Banco, Conexao, Resultado};

struct Pg(Client);

pub(crate) fn conectar(c: &Conexao, senha: &str) -> Result<Box<dyn Banco>, String> {
    let mut cfg = postgres::Config::new();
    cfg.host(&c.host).port(c.porta).user(&c.usuario).password(senha).application_name("FeatherDB");
    cfg.dbname(if c.banco.is_empty() { "postgres" } else { &c.banco });
    cfg.connect_timeout(std::time::Duration::from_secs(15));
    let cliente = if c.tls {
        let raizes = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_root_certificates(raizes)
            .with_no_client_auth();
        cfg.ssl_mode(postgres::config::SslMode::Require);
        cfg.connect(tokio_postgres_rustls::MakeRustlsConnect::new(tls))
    } else {
        cfg.connect(NoTls)
    };
    cliente.map(|c| Box::new(Pg(c)) as Box<dyn Banco>).map_err(|e| erro(&e))
}

/// A mensagem do servidor (`ERROR: ...`), sem o "db error:" do driver na frente.
fn erro(e: &postgres::Error) -> String {
    e.as_db_error().map_or_else(|| e.to_string(), |d| d.to_string())
}

impl Banco for Pg {
    fn executar(&mut self, sql: &str, limite: usize) -> Result<Resultado, String> {
        let mut r = Resultado::default();
        // Vários comandos de uma vez: fica o resultado do último que trouxe linhas.
        // shortcut: o protocolo simples traz todas as linhas para a memória antes de cortar no limite;
        // trocar por cursor (DECLARE/FETCH) quando tabelas de milhões de linhas pesarem.
        for m in self.0.simple_query(sql).map_err(|e| erro(&e))? {
            match m {
                SimpleQueryMessage::RowDescription(cols) => {
                    r = Resultado { colunas: cols.iter().map(|c| c.name().to_owned()).collect(), ..Resultado::default() };
                }
                SimpleQueryMessage::Row(linha) => {
                    if r.linhas.len() >= limite {
                        r.cortado = true;
                        continue;
                    }
                    r.linhas.push((0..linha.len()).map(|i| linha.get(i).map(str::to_owned)).collect());
                }
                SimpleQueryMessage::CommandComplete(n) if r.colunas.is_empty() => r.afetadas = Some(n),
                _ => {}
            }
        }
        Ok(r)
    }

    fn tabelas(&mut self) -> Result<Vec<crate::Tabela>, String> {
        crate::tabelas_do_catalogo(
            self,
        "SELECT table_schema, table_name, table_type FROM information_schema.tables \
         WHERE table_schema NOT IN ('pg_catalog', 'information_schema') ORDER BY 1, 2",
        )
    }
}

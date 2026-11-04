//! PostgreSQL pelo protocolo simples: o servidor devolve todo valor já como texto, de qualquer tipo.
use std::sync::Arc;

use postgres::{Client, NoTls, SimpleQueryMessage};

use crate::{Banco, Conexao, Resultado};

struct Pg(Client, Option<tokio_postgres_rustls::MakeRustlsConnect>);

pub(crate) fn conectar(c: &Conexao, senha: &str) -> Result<Box<dyn Banco>, String> {
    let mut cfg = postgres::Config::new();
    cfg.host(&c.host).port(c.porta).user(&c.usuario).password(senha).application_name("FeatherDB");
    cfg.dbname(if c.banco.is_empty() { "postgres" } else { &c.banco });
    cfg.connect_timeout(std::time::Duration::from_secs(15));
    let mut conector = None;
    let cliente = if c.tls {
        let raizes = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_root_certificates(raizes)
            .with_no_client_auth();
        cfg.ssl_mode(postgres::config::SslMode::Require);
        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(tls);
        conector = Some(tls.clone());
        cfg.connect(tls)
    } else {
        cfg.connect(NoTls)
    };
    cliente.map(|c| Box::new(Pg(c, conector)) as Box<dyn Banco>).map_err(|e| erro(&e))
}

/// A mensagem do servidor (`ERROR: ...`), sem o "db error:" do driver na frente.
fn erro(e: &postgres::Error) -> String {
    e.as_db_error().map_or_else(|| e.to_string(), |d| d.to_string())
}

impl Banco for Pg {
    fn cancelador(&self) -> Option<crate::Cancelar> {
        // O Postgres cancela por uma conexão à parte, com o token desta.
        let (token, tls) = (self.0.cancel_token(), self.1.clone());
        Some(std::sync::Arc::new(move || {
            let _ = match &tls {
                Some(t) => token.cancel_query(t.clone()),
                None => token.cancel_query(NoTls),
            };
        }))
    }

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
        if !r.colunas.is_empty() && crate::dividir(sql).len() == 1 {
            if let Ok(st) = self.0.prepare(sql) {
                r.tipos = st.columns().iter().map(|c| c.type_().name().to_owned()).collect();
            }
        }
        Ok(r)
    }

    fn colunas(&mut self, t: &crate::Tabela) -> Result<Vec<crate::Coluna>, String> {
        // pg_attribute dá o tipo como o Postgres escreve (varchar(120), numeric(12,2)) e a chave pelo índice primário.
        let sql = format!(
            "SELECT a.attname, format_type(a.atttypid, a.atttypmod), CASE WHEN a.attnotnull THEN 'NO' ELSE 'YES' END, \
             pg_get_expr(d.adbin, d.adrelid), \
             CASE WHEN EXISTS (SELECT 1 FROM pg_index i WHERE i.indrelid = a.attrelid AND i.indisprimary AND a.attnum = ANY(i.indkey)) THEN 'S' ELSE 'N' END \
             FROM pg_attribute a LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             WHERE a.attrelid = {}::regclass AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum",
            crate::literal(&t.qualificado(crate::Tipo::Postgres))
        );
        crate::colunas_do_catalogo(self, &sql)
    }

    fn ddl(&mut self, t: &crate::Tabela) -> Result<String, String> {
        let nome = t.qualificado(crate::Tipo::Postgres);
        if t.visao {
            let r = self.executar(&format!("SELECT pg_get_viewdef({}::regclass, true)", crate::literal(&nome)), 1)?;
            let corpo = r.linhas.into_iter().next().and_then(|l| l.into_iter().next().flatten()).unwrap_or_default();
            return Ok(format!("CREATE VIEW {nome} AS\n{}\n", corpo.trim_end()));
        }
        let colunas = self.colunas(t)?;
        Ok(crate::estrutura::ddl(crate::Tipo::Postgres, &nome, &colunas))
    }

    fn dicionario(&mut self) -> Result<Vec<crate::CampoDe>, String> {
        crate::dicionario_do_catalogo(
            self,
            "SELECT table_schema, table_name, column_name, data_type FROM information_schema.columns \
             WHERE table_schema NOT IN ('pg_catalog', 'information_schema') ORDER BY 1, 2, ordinal_position",
        )
    }

    fn bancos(&mut self) -> Result<Vec<String>, String> {
        let r = self.executar("SELECT datname FROM pg_database WHERE datallowconn AND NOT datistemplate ORDER BY 1", usize::MAX)?;
        Ok(r.linhas.into_iter().filter_map(|l| l.into_iter().next().flatten()).collect())
    }

    fn tabelas(&mut self) -> Result<Vec<crate::Tabela>, String> {
        crate::tabelas_do_catalogo(
            self,
        "SELECT table_schema, table_name, table_type FROM information_schema.tables \
         WHERE table_schema NOT IN ('pg_catalog', 'information_schema') ORDER BY 1, 2",
        )
    }
}

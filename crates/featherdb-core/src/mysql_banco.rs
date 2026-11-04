//! MySQL e MariaDB (driver 100% Rust).
use mysql::prelude::Queryable;
use mysql::{Conn, OptsBuilder, SslOpts, Value};

use crate::{Banco, Conexao, Resultado};

struct My(Conn, OptsBuilder);

pub(crate) fn conectar(c: &Conexao, senha: &str) -> Result<Box<dyn Banco>, String> {
    let opts = OptsBuilder::new()
        .ip_or_hostname(Some(c.host.clone()))
        .tcp_port(c.porta)
        .user(Some(c.usuario.clone()))
        .pass(Some(senha.to_owned()))
        .db_name((!c.banco.is_empty()).then(|| c.banco.clone()))
        .tcp_connect_timeout(Some(std::time::Duration::from_secs(15)))
        .ssl_opts(c.tls.then(SslOpts::default));
    Conn::new(opts.clone()).map(|c| Box::new(My(c, opts)) as Box<dyn Banco>).map_err(|e| e.to_string())
}

/// O valor como a pessoa espera ler (o protocolo de texto já manda quase tudo como bytes).
fn texto(v: Value) -> Option<String> {
    Some(match v {
        Value::NULL => return None,
        Value::Bytes(b) => String::from_utf8_lossy(&b).into_owned(),
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Double(d) => d.to_string(),
        Value::Date(a, m, d, h, mi, s, us) => {
            if (h, mi, s, us) == (0, 0, 0, 0) { format!("{a:04}-{m:02}-{d:02}") } else { format!("{a:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}") }
        }
        Value::Time(neg, dias, h, mi, s, _) => format!("{}{:02}:{mi:02}:{s:02}", if neg { "-" } else { "" }, dias * 24 + u32::from(h)),
    })
}

impl Banco for My {
    fn cancelador(&self) -> Option<crate::Cancelar> {
        // O MySQL para a consulta de outra conexão: KILL QUERY com o número desta.
        let (id, opts) = (self.0.connection_id(), self.1.clone());
        Some(std::sync::Arc::new(move || {
            if let Ok(mut c) = Conn::new(opts.clone()) {
                let _ = c.query_drop(format!("KILL QUERY {id}"));
            }
        }))
    }

    fn executar(&mut self, sql: &str, limite: usize) -> Result<Resultado, String> {
        let mut consulta = self.0.query_iter(sql).map_err(|e| e.to_string())?;
        let mut r = Resultado::default();
        // Vários comandos: fica o último conjunto com colunas (ou a contagem do último comando).
        while let Some(conjunto) = consulta.iter() {
            let colunas: Vec<String> = conjunto.columns().as_ref().iter().map(|c| c.name_str().into_owned()).collect();
            // MYSQL_TYPE_VAR_STRING vira var_string: o nome do tipo sem o prefixo.
            let tipos: Vec<String> = conjunto.columns().as_ref().iter().map(|c| format!("{:?}", c.column_type()).trim_start_matches("MYSQL_TYPE_").to_lowercase()).collect();
            let afetadas = conjunto.affected_rows();
            let mut atual = Resultado { colunas, tipos, ..Resultado::default() };
            for linha in conjunto {
                let linha = linha.map_err(|e| e.to_string())?;
                if atual.linhas.len() >= limite {
                    atual.cortado = true;
                    continue;
                }
                atual.linhas.push(linha.unwrap().into_iter().map(texto).collect());
            }
            if atual.colunas.is_empty() {
                atual.afetadas = Some(afetadas);
                if !r.colunas.is_empty() {
                    continue;
                }
            }
            r = atual;
        }
        Ok(r)
    }

    fn colunas(&mut self, t: &crate::Tabela) -> Result<Vec<crate::Coluna>, String> {
        let sql = format!(
            "SELECT column_name, column_type, is_nullable, column_default, CASE WHEN column_key = 'PRI' THEN 'S' ELSE 'N' END \
             FROM information_schema.columns WHERE table_schema = {} AND table_name = {} ORDER BY ordinal_position",
            crate::literal(&t.esquema),
            crate::literal(&t.nome)
        );
        crate::colunas_do_catalogo(self, &sql)
    }

    fn dicionario(&mut self) -> Result<Vec<crate::CampoDe>, String> {
        crate::dicionario_do_catalogo(
            self,
            "SELECT table_schema, table_name, column_name, column_type FROM information_schema.columns \
             WHERE table_schema = DATABASE() ORDER BY 1, 2, ordinal_position",
        )
    }

    fn bancos(&mut self) -> Result<Vec<String>, String> {
        let r = self.executar(
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys') ORDER BY 1",
            usize::MAX,
        )?;
        Ok(r.linhas.into_iter().filter_map(|l| l.into_iter().next().flatten()).collect())
    }

    fn tabelas(&mut self) -> Result<Vec<crate::Tabela>, String> {
        crate::tabelas_do_catalogo(
            self,
        "SELECT table_schema, table_name, table_type FROM information_schema.tables \
         WHERE table_schema = DATABASE() ORDER BY 1, 2",
        )
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn valores_como_texto() {
        assert_eq!(texto(Value::NULL), None);
        assert_eq!(texto(Value::Bytes("ção".into())).as_deref(), Some("ção"));
        assert_eq!(texto(Value::Date(2026, 11, 4, 0, 0, 0, 0)).as_deref(), Some("2026-11-04"));
        assert_eq!(texto(Value::Date(2026, 11, 4, 9, 5, 7, 0)).as_deref(), Some("2026-11-04 09:05:07"));
        assert_eq!(texto(Value::Time(true, 1, 2, 3, 4, 0)).as_deref(), Some("-26:03:04"));
    }
}

//! SQLite: um arquivo no disco, sem servidor.
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};

use crate::{Banco, Conexao, Resultado, Tabela};

struct Lite(Connection);

pub(crate) fn conectar(c: &Conexao) -> Result<Box<dyn Banco>, String> {
    if c.caminho.trim().is_empty() {
        return Err("Escolha o arquivo do banco SQLite.".into());
    }
    // Cria o arquivo se não existir: é assim que se começa um banco SQLite novo.
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_URI;
    Connection::open_with_flags(&c.caminho, flags).map(|c| Box::new(Lite(c)) as Box<dyn Banco>).map_err(|e| e.to_string())
}

fn texto(v: ValueRef<'_>) -> Option<String> {
    Some(match v {
        ValueRef::Null => return None,
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Blob(b) => format!("[{} bytes]", b.len()),
    })
}

impl Banco for Lite {
    fn executar(&mut self, sql: &str, limite: usize) -> Result<Resultado, String> {
        let erro = |e: rusqlite::Error| e.to_string();
        let comandos = crate::dividir(sql);
        let Some((ultimo, antes)) = comandos.split_last() else { return Ok(Resultado::default()) };
        for c in antes {
            self.0.execute_batch(c).map_err(erro)?;
        }
        let mut st = self.0.prepare(ultimo).map_err(erro)?;
        let colunas: Vec<String> = st.column_names().into_iter().map(str::to_owned).collect();
        if colunas.is_empty() {
            let n = st.execute([]).map_err(erro)?;
            return Ok(Resultado { afetadas: Some(n as u64), ..Resultado::default() });
        }
        let tipos = st.columns().iter().map(|c| c.decl_type().unwrap_or("").to_owned()).collect();
        let mut r = Resultado { colunas, tipos, ..Resultado::default() };
        let mut linhas = st.query([]).map_err(erro)?;
        while let Some(l) = linhas.next().map_err(erro)? {
            if r.linhas.len() >= limite {
                r.cortado = true;
                break;
            }
            r.linhas.push((0..r.colunas.len()).map(|i| l.get_ref(i).ok().and_then(texto)).collect());
        }
        Ok(r)
    }

    fn tabelas(&mut self) -> Result<Vec<Tabela>, String> {
        let r = self.executar("SELECT name, type FROM sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY name", usize::MAX)?;
        Ok(r.linhas
            .into_iter()
            .map(|l| Tabela { esquema: String::new(), nome: l[0].clone().unwrap_or_default(), visao: l[1].as_deref() == Some("view") })
            .collect())
    }

    fn colunas(&mut self, t: &Tabela) -> Result<Vec<crate::Coluna>, String> {
        let mut st = self.0.prepare("SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info(?1)").map_err(|e| e.to_string())?;
        let linhas = st
            .query_map([&t.nome], |l| {
                Ok(crate::Coluna { nome: l.get(0)?, tipo: l.get(1)?, nulo: l.get::<_, i64>(2)? == 0, padrao: l.get(3)?, chave: l.get::<_, i64>(4)? > 0 })
            })
            .map_err(|e| e.to_string())?;
        linhas.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }
}

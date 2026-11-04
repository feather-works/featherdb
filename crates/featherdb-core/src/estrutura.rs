//! A estrutura de uma tabela: as colunas com tipo, se aceitam vazio, o valor padrão e a chave; o
//! dicionário do banco (todas as colunas de todas as tabelas, para buscar e exportar); e o SQL para
//! criar ou mudar uma coluna, que a interface mostra antes de rodar.
use crate::{identificador, Tipo};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coluna {
    pub nome: String,
    /// Como o banco escreve o tipo: `integer`, `varchar(120)`, `timestamp`…
    pub tipo: String,
    /// Aceita NULL.
    pub nulo: bool,
    /// O valor padrão, como o banco escreve (`now()`, `'ativo'`, `0`).
    pub padrao: Option<String>,
    /// Faz parte da chave primária.
    pub chave: bool,
}

/// Uma linha do dicionário do banco: a coluna e a tabela dela.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CampoDe {
    pub esquema: String,
    pub tabela: String,
    pub coluna: String,
    pub tipo: String,
}

/// A família do tipo, para o ícone e o alinhamento (números à direita).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Familia {
    Numero,
    Texto,
    Data,
    Logico,
    Json,
    Binario,
    Outro,
}

pub fn familia(tipo: &str) -> Familia {
    let t = tipo.to_ascii_lowercase();
    let tem = |p: &[&str]| p.iter().any(|x| t.contains(x));
    if t == "objectid" || t == "uuid" {
        Familia::Texto
    } else if tem(&["bool", "bit"]) {
        Familia::Logico
    } else if tem(&["json", "object", "array", "[]", "document"]) {
        Familia::Json
    } else if tem(&["date", "time", "year", "interval"]) {
        Familia::Data
    } else if tem(&["int", "serial", "numeric", "decimal", "real", "double", "float", "money", "number", "long"]) {
        Familia::Numero
    } else if tem(&["char", "text", "string", "uuid", "enum", "citext", "name", "clob", "objectid"]) {
        Familia::Texto
    } else if tem(&["blob", "bytea", "binary", "raw", "image"]) {
        Familia::Binario
    } else {
        Familia::Outro
    }
}

/// Tipos que aparecem como sugestão no diálogo da coluna.
pub fn tipos_comuns(tipo: Tipo) -> &'static [&'static str] {
    match tipo {
        Tipo::Postgres => &["integer", "bigint", "numeric(12,2)", "text", "varchar(255)", "boolean", "date", "timestamp", "timestamptz", "uuid", "jsonb"],
        Tipo::MySql => &["int", "bigint", "decimal(12,2)", "varchar(255)", "text", "tinyint(1)", "date", "datetime", "timestamp", "json"],
        Tipo::Sqlite => &["INTEGER", "REAL", "TEXT", "BLOB", "NUMERIC"],
        Tipo::Odbc => &["INTEGER", "BIGINT", "DECIMAL(12,2)", "VARCHAR(255)", "DATE", "TIMESTAMP"],
        Tipo::MongoDb => &[],
    }
}

/// O valor padrão como vai no SQL: número, função (`now()`), palavra-chave e texto já entre aspas passam
/// como estão; o resto vira texto entre aspas.
fn padrao_sql(v: &str) -> String {
    let v = v.trim();
    let cru = v.parse::<f64>().is_ok()
        || v.starts_with('\'')
        || v.ends_with(')')
        || ["null", "true", "false", "current_timestamp", "current_date", "current_time"].contains(&v.to_ascii_lowercase().as_str());
    if cru { v.to_owned() } else { format!("'{}'", v.replace('\'', "''")) }
}

/// O SQL para criar a coluna `nova` na tabela.
pub fn sql_adicionar(tipo: Tipo, tabela: &str, nova: &Coluna) -> Result<Vec<String>, String> {
    if nova.nome.trim().is_empty() || nova.tipo.trim().is_empty() {
        return Err("Dê nome e tipo à coluna.".into());
    }
    if tipo == Tipo::MongoDb {
        return Err("No MongoDB os campos não têm estrutura fixa: basta gravar documentos com o campo novo.".into());
    }
    let mut s = format!("ALTER TABLE {tabela} ADD COLUMN {} {}", identificador(nova.nome.trim(), tipo), nova.tipo.trim());
    if !nova.nulo {
        s.push_str(" NOT NULL");
    }
    if let Some(p) = nova.padrao.as_deref().filter(|p| !p.trim().is_empty()) {
        s.push_str(&format!(" DEFAULT {}", padrao_sql(p)));
    }
    Ok(vec![s])
}

/// O SQL para levar a coluna de `antes` para `depois`. Vazio se nada mudou.
pub fn sql_alterar(tipo: Tipo, tabela: &str, antes: &Coluna, depois: &Coluna) -> Result<Vec<String>, String> {
    let nome = depois.nome.trim();
    if nome.is_empty() || depois.tipo.trim().is_empty() {
        return Err("Dê nome e tipo à coluna.".into());
    }
    let q = |n: &str| identificador(n, tipo);
    let padrao_novo = depois.padrao.as_deref().map(str::trim).filter(|p| !p.is_empty());
    let padrao_antes = antes.padrao.as_deref().map(str::trim).filter(|p| !p.is_empty());
    let mudou_tipo = depois.tipo.trim() != antes.tipo.trim();
    let mut s = Vec::new();
    match tipo {
        Tipo::Postgres => {
            if nome != antes.nome {
                s.push(format!("ALTER TABLE {tabela} RENAME COLUMN {} TO {}", q(&antes.nome), q(nome)));
            }
            if mudou_tipo {
                // O USING converte os dados que já estão lá (texto "12" vira número 12, por exemplo).
                s.push(format!("ALTER TABLE {tabela} ALTER COLUMN {} TYPE {} USING {}::{}", q(nome), depois.tipo.trim(), q(nome), depois.tipo.trim()));
            }
            if depois.nulo != antes.nulo {
                s.push(format!("ALTER TABLE {tabela} ALTER COLUMN {} {} NOT NULL", q(nome), if depois.nulo { "DROP" } else { "SET" }));
            }
            if padrao_novo != padrao_antes {
                s.push(match padrao_novo {
                    Some(p) => format!("ALTER TABLE {tabela} ALTER COLUMN {} SET DEFAULT {}", q(nome), padrao_sql(p)),
                    None => format!("ALTER TABLE {tabela} ALTER COLUMN {} DROP DEFAULT", q(nome)),
                });
            }
        }
        Tipo::MySql => {
            // O MySQL muda tudo de uma vez: nome, tipo, vazio e padrão na mesma linha.
            if nome != antes.nome || mudou_tipo || depois.nulo != antes.nulo || padrao_novo != padrao_antes {
                let mut linha = format!("ALTER TABLE {tabela} CHANGE COLUMN {} {} {}", q(&antes.nome), q(nome), depois.tipo.trim());
                linha.push_str(if depois.nulo { " NULL" } else { " NOT NULL" });
                if let Some(p) = padrao_novo {
                    linha.push_str(&format!(" DEFAULT {}", padrao_sql(p)));
                }
                s.push(linha);
            }
        }
        Tipo::Sqlite => {
            if mudou_tipo || depois.nulo != antes.nulo || padrao_novo != padrao_antes {
                return Err("O SQLite só deixa trocar o nome de uma coluna. Para mudar tipo, vazio ou padrão é preciso recriar a tabela.".into());
            }
            if nome != antes.nome {
                s.push(format!("ALTER TABLE {tabela} RENAME COLUMN {} TO {}", q(&antes.nome), q(nome)));
            }
        }
        Tipo::Odbc => return Err("Pelo ODBC cada banco muda coluna de um jeito: escreva o ALTER TABLE no editor.".into()),
        Tipo::MongoDb => return Err("No MongoDB os campos não têm estrutura fixa.".into()),
    }
    Ok(s)
}

/// O valor como literal SQL (NULL, ou texto entre aspas; no MySQL a barra invertida vai dobrada).
pub fn valor_sql(tipo: Tipo, v: Option<&str>) -> String {
    match v {
        None => "NULL".into(),
        Some(v) if tipo == Tipo::MySql => crate::literal(&v.replace('\\', "\\\\")),
        Some(v) => crate::literal(v),
    }
}

/// O INSERT de uma linha nova da grade, só com as colunas que a pessoa preencheu (as outras ficam com o padrão).
pub fn sql_inserir(tipo: Tipo, tabela: &str, colunas: &[String], valores: &[(usize, Option<String>)]) -> Option<String> {
    if valores.is_empty() {
        return None;
    }
    let nomes: Vec<String> = valores.iter().map(|(c, _)| identificador(&colunas[*c], tipo)).collect();
    let vals: Vec<String> = valores.iter().map(|(_, v)| valor_sql(tipo, v.as_deref())).collect();
    Some(format!("INSERT INTO {tabela} ({}) VALUES ({})", nomes.join(", "), vals.join(", ")))
}

/// O DELETE de uma linha pela chave primária. `None` quando a chave é vazia.
pub fn sql_excluir(tipo: Tipo, tabela: &str, colunas: &[String], chaves: &[usize], linha: &[Option<String>]) -> Option<String> {
    let mut onde = Vec::new();
    for &k in chaves {
        onde.push(format!("{} = {}", identificador(&colunas[k], tipo), valor_sql(tipo, Some(linha.get(k)?.as_deref()?))));
    }
    (!onde.is_empty()).then(|| format!("DELETE FROM {tabela} WHERE {}", onde.join(" AND ")))
}

/// O DROP de uma tabela, view ou view materializada.
pub fn sql_apagar(tipo: Tipo, t: &crate::Tabela) -> Result<String, String> {
    let objeto = match (t.materializada, t.visao) {
        (true, _) => "MATERIALIZED VIEW",
        (_, true) => "VIEW",
        _ => "TABLE",
    };
    match tipo {
        Tipo::MongoDb => Err("No MongoDB apague a coleção pelo editor: db.colecao.drop() não é aceito aqui.".into()),
        _ => Ok(format!("DROP {objeto} {}", t.qualificado(tipo))),
    }
}

/// O SQL que troca o nome de uma tabela, view ou view materializada (o esquema continua o mesmo).
pub fn sql_renomear(tipo: Tipo, t: &crate::Tabela, novo: &str) -> Result<String, String> {
    let novo = novo.trim();
    if novo.is_empty() {
        return Err("Escreva o nome novo.".into());
    }
    let (atual, n) = (t.qualificado(tipo), identificador(novo, tipo));
    match tipo {
        Tipo::Postgres => {
            let objeto = if t.materializada { "MATERIALIZED VIEW" } else if t.visao { "VIEW" } else { "TABLE" };
            Ok(format!("ALTER {objeto} {atual} RENAME TO {n}"))
        }
        // No MySQL o nome novo leva o banco junto, senão a tabela muda para o banco padrão.
        Tipo::MySql => Ok(format!("RENAME TABLE {atual} TO {}", crate::Tabela { nome: novo.to_owned(), ..t.clone() }.qualificado(tipo))),
        Tipo::Sqlite if t.visao => Err("O SQLite não renomeia view: apague e crie de novo com o nome novo.".into()),
        Tipo::Sqlite => Ok(format!("ALTER TABLE {atual} RENAME TO {n}")),
        Tipo::Odbc | Tipo::MongoDb => Err("Renomeie escrevendo o comando no editor.".into()),
    }
}

/// O UPDATE de uma linha editada na grade: as colunas mudadas no SET e a chave primária no WHERE
/// (com o valor que a linha tinha). `None` quando a chave é vazia (NULL): sem ela não dá para achar a linha.
pub fn sql_atualizar(tipo: Tipo, tabela: &str, colunas: &[String], chaves: &[usize], linha: &[Option<String>], mudancas: &[(usize, Option<String>)]) -> Option<String> {
    let q = |n: &str| identificador(n, tipo);
    // No MySQL a barra invertida é escape dentro das aspas: vai dobrada para gravar como está.
    let texto = |v: &str| valor_sql(tipo, Some(v));
    let valor = |v: &Option<String>| valor_sql(tipo, v.as_deref());
    let set: Vec<String> = mudancas.iter().map(|(c, v)| format!("{} = {}", q(&colunas[*c]), valor(v))).collect();
    let mut onde = Vec::new();
    for &k in chaves {
        onde.push(format!("{} = {}", q(&colunas[k]), texto(linha.get(k)?.as_deref()?)));
    }
    (!set.is_empty() && !onde.is_empty()).then(|| format!("UPDATE {tabela} SET {} WHERE {}", set.join(", "), onde.join(" AND ")))
}

/// O `CREATE TABLE` montado pelas colunas (o padrão vem do catálogo já como expressão SQL).
pub fn ddl(tipo: Tipo, tabela: &str, colunas: &[Coluna]) -> String {
    let q = |n: &str| identificador(n, tipo);
    let mut linhas: Vec<String> = colunas
        .iter()
        .map(|c| {
            let mut l = format!("  {} {}", q(&c.nome), c.tipo);
            if !c.nulo {
                l.push_str(" NOT NULL");
            }
            if let Some(p) = c.padrao.as_deref().filter(|p| !p.trim().is_empty()) {
                l.push_str(&format!(" DEFAULT {p}"));
            }
            l
        })
        .collect();
    let chave: Vec<String> = colunas.iter().filter(|c| c.chave).map(|c| q(&c.nome)).collect();
    if !chave.is_empty() {
        linhas.push(format!("  PRIMARY KEY ({})", chave.join(", ")));
    }
    format!("CREATE TABLE {tabela} (\n{}\n);\n", linhas.join(",\n"))
}

/// O texto separado (CSV ou TSV para a planilha) de uma lista de colunas.
pub fn colunas_em_texto(colunas: &[Coluna], sep: char) -> String {
    let campo = |s: &str| if s.contains([sep, '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_owned() };
    let mut saida = ["coluna", "tipo", "aceita_vazio", "padrao", "chave"].join(&sep.to_string());
    for c in colunas {
        let linha = [c.nome.as_str(), c.tipo.as_str(), if c.nulo { "sim" } else { "não" }, c.padrao.as_deref().unwrap_or(""), if c.chave { "sim" } else { "" }];
        saida.push_str("\r\n");
        saida.push_str(&linha.iter().map(|s| campo(s)).collect::<Vec<_>>().join(&sep.to_string()));
    }
    saida.push_str("\r\n");
    saida
}

/// O dicionário do banco em texto separado.
pub fn dicionario_em_texto(campos: &[CampoDe], sep: char) -> String {
    let campo = |s: &str| if s.contains([sep, '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_owned() };
    let mut saida = ["esquema", "tabela", "coluna", "tipo"].join(&sep.to_string());
    for c in campos {
        saida.push_str("\r\n");
        saida.push_str(&[&c.esquema, &c.tabela, &c.coluna, &c.tipo].iter().map(|s| campo(s)).collect::<Vec<_>>().join(&sep.to_string()));
    }
    saida.push_str("\r\n");
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    fn col(nome: &str, tipo: &str, nulo: bool, padrao: Option<&str>) -> Coluna {
        Coluna { nome: nome.into(), tipo: tipo.into(), nulo, padrao: padrao.map(Into::into), chave: false }
    }

    #[test]
    fn familias() {
        assert_eq!(familia("character varying(120)"), Familia::Texto);
        assert_eq!(familia("integer"), Familia::Numero);
        assert_eq!(familia("timestamp without time zone"), Familia::Data);
        assert_eq!(familia("jsonb"), Familia::Json);
        assert_eq!(familia("tinyint(1)"), Familia::Numero);
        assert_eq!(familia("boolean"), Familia::Logico);
        assert_eq!(familia("objectId"), Familia::Texto);
    }

    #[test]
    fn alterar_no_postgres_so_o_que_mudou() {
        let antes = col("nome", "varchar(50)", true, None);
        let depois = col("nome_completo", "varchar(120)", false, Some("sem nome"));
        assert_eq!(
            sql_alterar(Tipo::Postgres, "public.cliente", &antes, &depois).unwrap(),
            vec![
                "ALTER TABLE public.cliente RENAME COLUMN nome TO nome_completo",
                "ALTER TABLE public.cliente ALTER COLUMN nome_completo TYPE varchar(120) USING nome_completo::varchar(120)",
                "ALTER TABLE public.cliente ALTER COLUMN nome_completo SET NOT NULL",
                "ALTER TABLE public.cliente ALTER COLUMN nome_completo SET DEFAULT 'sem nome'",
            ]
        );
        assert!(sql_alterar(Tipo::Postgres, "t", &antes, &antes).unwrap().is_empty());
    }

    #[test]
    fn alterar_no_mysql_e_no_sqlite() {
        let antes = col("criado", "datetime", true, None);
        let depois = col("criado_em", "datetime", false, Some("CURRENT_TIMESTAMP"));
        assert_eq!(sql_alterar(Tipo::MySql, "loja.pedido", &antes, &depois).unwrap(), vec!["ALTER TABLE loja.pedido CHANGE COLUMN criado criado_em datetime NOT NULL DEFAULT CURRENT_TIMESTAMP"]);
        assert!(sql_alterar(Tipo::Sqlite, "t", &antes, &depois).is_err());
        let so_nome = col("criado_em", "datetime", true, None);
        assert_eq!(sql_alterar(Tipo::Sqlite, "t", &antes, &so_nome).unwrap(), vec!["ALTER TABLE t RENAME COLUMN criado TO criado_em"]);
    }

    #[test]
    fn ddl_com_chave_e_padrao() {
        let c = |nome: &str, tipo: &str, nulo: bool, padrao: Option<&str>, chave: bool| Coluna { nome: nome.into(), tipo: tipo.into(), nulo, padrao: padrao.map(Into::into), chave };
        let cols = [c("id", "integer", false, Some("nextval('t_id_seq'::regclass)"), true), c("Nome", "varchar(80)", true, None, false)];
        assert_eq!(
            ddl(Tipo::Postgres, "public.t", &cols),
            "CREATE TABLE public.t (\n  id integer NOT NULL DEFAULT nextval('t_id_seq'::regclass),\n  \"Nome\" varchar(80),\n  PRIMARY KEY (id)\n);\n"
        );
    }

    #[test]
    fn update_da_linha_editada() {
        let cols = ["id".to_owned(), "Nome".to_owned(), "nota".to_owned()];
        let linha = [Some("7".to_owned()), Some("Ana".to_owned()), None];
        assert_eq!(
            sql_atualizar(Tipo::Postgres, "public.t", &cols, &[0], &linha, &[(1, Some("O'Neil".into())), (2, None)]).as_deref(),
            Some("UPDATE public.t SET \"Nome\" = 'O''Neil', nota = NULL WHERE id = '7'")
        );
        assert_eq!(sql_atualizar(Tipo::MySql, "t", &cols, &[2], &linha, &[(1, None)]), None);
        assert_eq!(sql_atualizar(Tipo::MySql, "t", &cols, &[0], &linha, &[(1, Some("C:\\x".into()))]).as_deref(), Some("UPDATE t SET `Nome` = 'C:\\\\x' WHERE id = '7'"));
    }

    #[test]
    fn renomear_cada_banco_do_seu_jeito() {
        let t = crate::Tabela { esquema: "public".into(), nome: "vendas".into(), ..Default::default() };
        assert_eq!(sql_renomear(Tipo::Postgres, &t, "Vendas 2026").unwrap(), "ALTER TABLE public.vendas RENAME TO \"Vendas 2026\"");
        let m = crate::Tabela { visao: true, materializada: true, ..t.clone() };
        assert_eq!(sql_renomear(Tipo::Postgres, &m, "resumo").unwrap(), "ALTER MATERIALIZED VIEW public.vendas RENAME TO resumo");
        let my = crate::Tabela { esquema: "loja".into(), ..t.clone() };
        assert_eq!(sql_renomear(Tipo::MySql, &my, "pedidos").unwrap(), "RENAME TABLE loja.vendas TO loja.pedidos");
        assert!(sql_renomear(Tipo::Sqlite, &crate::Tabela { visao: true, ..t.clone() }, "x").is_err());
        assert!(sql_renomear(Tipo::Postgres, &t, "  ").is_err());
    }

    #[test]
    fn inserir_e_excluir_linha() {
        let cols = ["id".to_owned(), "nome".to_owned()];
        assert_eq!(sql_inserir(Tipo::Postgres, "t", &cols, &[(1, Some("Ana".into()))]).as_deref(), Some("INSERT INTO t (nome) VALUES ('Ana')"));
        assert_eq!(sql_inserir(Tipo::Postgres, "t", &cols, &[]), None);
        assert_eq!(sql_excluir(Tipo::Postgres, "t", &cols, &[0], &[Some("3".into()), None]).as_deref(), Some("DELETE FROM t WHERE id = '3'"));
        assert_eq!(sql_excluir(Tipo::Postgres, "t", &cols, &[1], &[Some("3".into()), None]), None);
    }

    #[test]
    fn adicionar_e_exportar() {
        assert_eq!(
            sql_adicionar(Tipo::Postgres, "public.user", &col("Ativo", "boolean", false, Some("true"))).unwrap(),
            vec!["ALTER TABLE public.user ADD COLUMN \"Ativo\" boolean NOT NULL DEFAULT true"]
        );
        assert!(sql_adicionar(Tipo::Postgres, "t", &col("", "int", true, None)).is_err());
        let texto = colunas_em_texto(&[Coluna { chave: true, ..col("id", "integer", false, Some("nextval('s')")) }], ',');
        assert_eq!(texto, "coluna,tipo,aceita_vazio,padrao,chave\r\nid,integer,não,nextval('s'),sim\r\n");
    }
}

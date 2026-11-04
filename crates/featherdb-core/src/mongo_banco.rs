//! MongoDB: no editor vão os comandos do shell do Mongo (`db.colecao.find({...})`, `aggregate`,
//! `countDocuments`, `show collections`, `show dbs`); cada documento vira uma linha da grade e cada campo
//! do primeiro nível, uma coluna (objetos e listas aparecem como JSON).
use std::time::Duration;

use mongodb::bson::{Bson, Document};
use mongodb::options::ClientOptions;
use mongodb::sync::{Client, Database};

use crate::{Banco, Conexao, Resultado, Tabela};

struct Mongo {
    cliente: Client,
    banco: Option<Database>,
}

/// Bancos internos do servidor, fora da árvore.
const DO_SISTEMA: [&str; 3] = ["admin", "local", "config"];

pub(crate) fn conectar(c: &Conexao, senha: &str) -> Result<Box<dyn Banco>, String> {
    let uri = com_senha(&c.caminho, senha)?;
    let mut opcoes = ClientOptions::parse(&uri).run().map_err(|e| e.to_string())?;
    opcoes.app_name = Some("FeatherDB".into());
    opcoes.server_selection_timeout = Some(Duration::from_secs(15));
    opcoes.connect_timeout = Some(Duration::from_secs(15));
    let padrao = opcoes.default_database.clone();
    let cliente = Client::with_options(opcoes).map_err(|e| e.to_string())?;
    // O driver só conecta de verdade no primeiro comando: o ping já mostra senha errada ou IP bloqueado.
    cliente.database("admin").run_command(mongodb::bson::doc! { "ping": 1 }).run().map_err(|e| erro(&e))?;
    let nome = if c.banco.trim().is_empty() { padrao } else { Some(c.banco.trim().to_owned()) };
    let banco = nome.map(|n| cliente.database(&n));
    Ok(Box::new(Mongo { cliente, banco }))
}

fn erro(e: &mongodb::error::Error) -> String {
    let texto = e.to_string();
    if texto.contains("Authentication failed") || texto.contains("bad auth") {
        return "Usuário ou senha do banco errados (no Atlas é a senha do usuário do banco, em Database Access, não a do site).".into();
    }
    if texto.contains("Server selection timeout") {
        return format!("O servidor não respondeu. No Atlas, libere o seu IP em Network Access.\n{texto}");
    }
    texto
}

/// Põe a senha no endereço (`usuario@` vira `usuario:senha@`), trocando o `<db_password>` do Atlas.
pub(crate) fn com_senha(uri: &str, senha: &str) -> Result<String, String> {
    let uri = uri.trim();
    let Some((esquema, resto)) = uri.split_once("://") else { return Err("O endereço precisa começar com mongodb:// ou mongodb+srv://.".into()) };
    let fim_host = resto.find('/').unwrap_or(resto.len());
    let Some(arroba) = resto[..fim_host].rfind('@') else { return Ok(uri.to_owned()) };
    let (usuario, atual) = resto[..arroba].split_once(':').unwrap_or((&resto[..arroba], ""));
    let senha = if senha.is_empty() { atual } else { senha };
    if senha.is_empty() || senha == "<db_password>" {
        return Err("Falta a senha do usuário do banco.".into());
    }
    let codificada: String = if senha.contains('%') && senha == atual {
        senha.to_owned()
    } else {
        senha.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
    };
    Ok(format!("{esquema}://{usuario}:{codificada}@{}", &resto[arroba + 1..]))
}

/// Um comando do editor, já lido.
#[derive(Debug, PartialEq)]
enum Comando {
    Find { colecao: String, filtro: Document, projecao: Option<Document>, ordem: Option<Document>, limite: Option<i64>, pular: Option<u64> },
    Aggregate { colecao: String, etapas: Vec<Document> },
    Contar { colecao: String, filtro: Document },
    Colecoes,
    Bancos,
}

/// Lê `db.colecao.metodo(args).sort(...).limit(n)`, `db.getCollection("x")...`, `show collections`, `show dbs`.
fn interpretar(texto: &str) -> Result<Comando, String> {
    let t = texto.trim().trim_end_matches(';').trim();
    match t.to_ascii_lowercase().as_str() {
        "show collections" | "show tables" | "db.getcollectionnames()" => return Ok(Comando::Colecoes),
        "show dbs" | "show databases" => return Ok(Comando::Bancos),
        _ => {}
    }
    let resto = t.strip_prefix("db.").ok_or("Comando do MongoDB começa com db. (ex.: db.clientes.find({})) ou show collections.")?;
    let (colecao, resto) = if let Some(r) = resto.strip_prefix("getCollection(") {
        let (arg, r) = ate_fechar(r)?;
        let nome: String = json5::from_str(arg.trim()).map_err(|_| "getCollection precisa do nome entre aspas.")?;
        (nome, r.strip_prefix('.').ok_or("Falta o método depois de getCollection(...), como .find({}).")?)
    } else {
        let fim = resto.find('.').ok_or("Falta o método, como .find({}).")?;
        (resto[..fim].to_owned(), &resto[fim + 1..])
    };
    let mut chamadas = Vec::new();
    let mut resto = resto;
    while !resto.trim().is_empty() {
        let r = resto.trim().trim_start_matches('.');
        let abre = r.find('(').ok_or_else(|| format!("Esperava (...) depois de {r}"))?;
        let (args, depois) = ate_fechar(&r[abre + 1..])?;
        chamadas.push((r[..abre].trim().to_owned(), argumentos(args)?));
        resto = depois;
    }
    let mut chamadas = chamadas.into_iter();
    let (metodo, mut args) = chamadas.next().ok_or("Falta o método, como .find({}).")?;
    let doc = |b: Option<Bson>| -> Result<Option<Document>, String> {
        match b {
            None => Ok(None),
            Some(Bson::Document(d)) => Ok(Some(d)),
            Some(_) => Err("Esperava um objeto { ... }.".into()),
        }
    };
    let mut primeiro = || if args.is_empty() { None } else { Some(args.remove(0)) };
    let mut c = match metodo.as_str() {
        "find" | "findOne" => {
            let filtro = doc(primeiro())?.unwrap_or_default();
            let projecao = doc(primeiro())?;
            Comando::Find { colecao, filtro, projecao, ordem: None, limite: (metodo == "findOne").then_some(1), pular: None }
        }
        "aggregate" => match primeiro() {
            Some(Bson::Array(v)) => Comando::Aggregate { colecao, etapas: v.into_iter().map(|e| doc(Some(e)).map(Option::unwrap_or_default)).collect::<Result<_, _>>()? },
            _ => return Err("aggregate precisa de uma lista de etapas: aggregate([ {...}, {...} ]).".into()),
        },
        "countDocuments" | "count" => Comando::Contar { colecao, filtro: doc(primeiro())?.unwrap_or_default() },
        outro => return Err(format!("O FeatherDB ainda não roda o método {outro}. Use find, findOne, aggregate ou countDocuments.")),
    };
    for (nome, mut a) in chamadas {
        let Comando::Find { ordem, limite, pular, .. } = &mut c else { return Err(format!(".{nome}() só vale depois de find().")) };
        let valor = if a.is_empty() { None } else { Some(a.remove(0)) };
        let numero = |v: Option<Bson>| v.and_then(|v| v.as_i64().or_else(|| v.as_i32().map(i64::from)).or_else(|| v.as_f64().map(|f| f as i64)));
        match nome.as_str() {
            "sort" => *ordem = doc(valor)?,
            "limit" => *limite = numero(valor),
            "skip" => *pular = numero(valor).map(|n| n.max(0) as u64),
            "pretty" | "toArray" => {}
            outro => return Err(format!("O FeatherDB ainda não entende .{outro}(). Use sort, limit ou skip.")),
        }
    }
    Ok(c)
}

/// O texto até o `)` que fecha o `(` já aberto, respeitando aspas e parênteses dentro; e o que vem depois.
fn ate_fechar(s: &str) -> Result<(&str, &str), String> {
    let (mut nivel, mut aspas) = (0i32, None::<char>);
    let mut anterior = ' ';
    for (i, c) in s.char_indices() {
        match (aspas, c) {
            (Some(q), c) if c == q && anterior != '\\' => aspas = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => aspas = Some(c),
            (None, '(' | '[' | '{') => nivel += 1,
            (None, ')') if nivel == 0 => return Ok((&s[..i], &s[i + 1..])),
            (None, ')' | ']' | '}') => nivel -= 1,
            _ => {}
        }
        anterior = c;
    }
    Err("Falta fechar um parêntese.".into())
}

/// Os argumentos da chamada, no jeito do shell: `{idade: {$gt: 30}}, {nome: 1}`, com `ObjectId("...")`
/// e `ISODate("...")`.
fn argumentos(texto: &str) -> Result<Vec<Bson>, String> {
    if texto.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut t = texto.to_owned();
    for (funcao, chave) in [("ObjectId(", "$oid"), ("ISODate(", "$date"), ("new Date(", "$date")] {
        while let Some(i) = t.find(funcao) {
            let (arg, _) = ate_fechar(&t[i + funcao.len()..])?;
            let fim = i + funcao.len() + arg.len() + 1;
            let novo = format!("{{\"{chave}\": {}}}", arg.trim());
            t.replace_range(i..fim, &novo);
        }
    }
    let valor: serde_json::Value = json5::from_str(&format!("[{t}]")).map_err(|e| format!("Não entendi os argumentos: {e}"))?;
    let serde_json::Value::Array(itens) = valor else { unreachable!() };
    itens.into_iter().map(|v| Bson::try_from(v).map_err(|e| e.to_string())).collect()
}

/// Um valor da grade: texto simples quando dá; objeto, lista e tipos do Mongo em JSON.
fn texto(v: &Bson) -> Option<String> {
    Some(match v {
        Bson::Null | Bson::Undefined => return None,
        Bson::String(s) => s.clone(),
        Bson::Int32(i) => i.to_string(),
        Bson::Int64(i) => i.to_string(),
        Bson::Double(d) => d.to_string(),
        Bson::Boolean(b) => b.to_string(),
        Bson::ObjectId(o) => o.to_hex(),
        Bson::DateTime(d) => d.try_to_rfc3339_string().unwrap_or_else(|_| d.to_string()),
        Bson::Decimal128(d) => d.to_string(),
        outro => outro.clone().into_relaxed_extjson().to_string(),
    })
}

fn tipo(v: &Bson) -> &'static str {
    match v {
        Bson::String(_) => "string",
        Bson::Int32(_) => "int",
        Bson::Int64(_) => "long",
        Bson::Double(_) => "double",
        Bson::Decimal128(_) => "decimal",
        Bson::Boolean(_) => "bool",
        Bson::ObjectId(_) => "objectId",
        Bson::DateTime(_) => "date",
        Bson::Document(_) => "object",
        Bson::Array(_) => "array",
        Bson::Null => "null",
        _ => "outro",
    }
}

/// Documentos na grade: as colunas na ordem em que os campos aparecem.
fn em_grade(docs: impl Iterator<Item = Result<Document, String>>, limite: usize) -> Result<Resultado, String> {
    let mut r = Resultado::default();
    let mut lidos = Vec::new();
    for d in docs {
        let d = d?;
        if lidos.len() >= limite {
            r.cortado = true;
            break;
        }
        for k in d.keys() {
            if !r.colunas.iter().any(|c| c == k) {
                r.colunas.push(k.clone());
            }
        }
        lidos.push(d);
    }
    r.linhas = lidos.iter().map(|d| r.colunas.iter().map(|c| d.get(c).and_then(texto)).collect()).collect();
    Ok(r)
}

impl Mongo {
    fn banco(&self) -> Result<&Database, String> {
        self.banco.as_ref().ok_or_else(|| "Escolha um banco na árvore à esquerda (ou ponha o banco no endereço).".to_owned())
    }
}

impl Banco for Mongo {
    fn executar(&mut self, comando: &str, limite: usize) -> Result<Resultado, String> {
        let um = |nome: &str, valor: String| Resultado { colunas: vec![nome.into()], linhas: vec![vec![Some(valor)]], ..Resultado::default() };
        match interpretar(comando)? {
            Comando::Bancos => {
                let nomes = self.cliente.list_database_names().run().map_err(|e| erro(&e))?;
                Ok(Resultado { colunas: vec!["banco".into()], linhas: nomes.into_iter().map(|n| vec![Some(n)]).collect(), ..Resultado::default() })
            }
            Comando::Colecoes => {
                let mut nomes = self.banco()?.list_collection_names().run().map_err(|e| erro(&e))?;
                nomes.sort();
                Ok(Resultado { colunas: vec!["coleção".into()], linhas: nomes.into_iter().map(|n| vec![Some(n)]).collect(), ..Resultado::default() })
            }
            Comando::Contar { colecao, filtro } => {
                let n = self.banco()?.collection::<Document>(&colecao).count_documents(filtro).run().map_err(|e| erro(&e))?;
                Ok(um("total", n.to_string()))
            }
            Comando::Find { colecao, filtro, projecao, ordem, limite: lim, pular } => {
                // Pede um a mais que o limite da tela para saber se ficou coisa de fora.
                let maximo = lim.map_or(limite as i64 + 1, |l| l.min(limite as i64 + 1));
                let mut opcoes = mongodb::options::FindOptions::default();
                opcoes.projection = projecao;
                opcoes.sort = ordem;
                opcoes.skip = pular;
                opcoes.limit = Some(maximo);
                let cursor = self.banco()?.collection::<Document>(&colecao).find(filtro).with_options(opcoes).run().map_err(|e| erro(&e))?;
                em_grade(cursor.map(|d| d.map_err(|e| erro(&e))), limite)
            }
            Comando::Aggregate { colecao, etapas } => {
                let cursor = self.banco()?.collection::<Document>(&colecao).aggregate(etapas).run().map_err(|e| erro(&e))?;
                em_grade(cursor.take(limite + 1).map(|d| d.map_err(|e| erro(&e))), limite)
            }
        }
    }

    fn tabelas(&mut self) -> Result<Vec<Tabela>, String> {
        let mut nomes = self.banco()?.list_collection_names().run().map_err(|e| erro(&e))?;
        nomes.sort();
        Ok(nomes.into_iter().filter(|n| !n.starts_with("system.")).map(|nome| Tabela { esquema: String::new(), nome, visao: false }).collect())
    }

    /// O Mongo não tem colunas fixas: os campos dos primeiros 50 documentos, com o tipo do primeiro que aparece.
    fn colunas(&mut self, t: &Tabela) -> Result<Vec<(String, String)>, String> {
        let cursor = self.banco()?.collection::<Document>(&t.nome).find(Document::new()).limit(50).run().map_err(|e| erro(&e))?;
        let mut campos: Vec<(String, String)> = Vec::new();
        for d in cursor {
            for (k, v) in d.map_err(|e| erro(&e))? {
                if !campos.iter().any(|(c, _)| *c == k) {
                    campos.push((k, tipo(&v).into()));
                }
            }
        }
        Ok(campos)
    }

    fn bancos(&mut self) -> Result<Vec<String>, String> {
        let nomes = self.cliente.list_database_names().run().map_err(|e| erro(&e))?;
        Ok(nomes.into_iter().filter(|n| !DO_SISTEMA.contains(&n.as_str())).collect())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use mongodb::bson::doc;

    #[test]
    fn le_os_comandos_do_shell() {
        assert_eq!(
            interpretar("db.clientes.find({idade: {$gt: 30}}, {nome: 1}).sort({nome: -1}).limit(5);").unwrap(),
            Comando::Find { colecao: "clientes".into(), filtro: doc! {"idade": {"$gt": 30}}, projecao: Some(doc! {"nome": 1}), ordem: Some(doc! {"nome": -1}), limite: Some(5), pular: None }
        );
        assert_eq!(
            interpretar("db.getCollection('meus pedidos').countDocuments({status: 'pago'})").unwrap(),
            Comando::Contar { colecao: "meus pedidos".into(), filtro: doc! {"status": "pago"} }
        );
        let Comando::Find { filtro, .. } = interpretar(r#"db.x.find({_id: ObjectId("65a1b2c3d4e5f60718293a4b")})"#).unwrap() else { panic!() };
        assert_eq!(filtro.get_object_id("_id").unwrap().to_hex(), "65a1b2c3d4e5f60718293a4b");
        let Comando::Aggregate { etapas, .. } = interpretar("db.vendas.aggregate([{$match: {ano: 2026}}, {$group: {_id: '$loja', total: {$sum: '$valor'}}}])").unwrap() else { panic!() };
        assert_eq!(etapas.len(), 2);
        assert_eq!(interpretar("show collections").unwrap(), Comando::Colecoes);
        assert!(interpretar("select * from x").is_err());
        assert!(interpretar("db.x.drop()").is_err());
    }

    #[test]
    fn senha_no_endereco() {
        assert_eq!(com_senha("mongodb+srv://ana:<db_password>@c.x.mongodb.net/", "p@ss w").unwrap(), "mongodb+srv://ana:p%40ss%20w@c.x.mongodb.net/");
        assert_eq!(com_senha("mongodb://ana@h:27017/loja?tls=true", "s").unwrap(), "mongodb://ana:s@h:27017/loja?tls=true");
        assert_eq!(com_senha("mongodb://localhost:27017", "").unwrap(), "mongodb://localhost:27017");
        assert!(com_senha("mongodb+srv://ana:<db_password>@c.x.mongodb.net/", "").is_err());
    }

    #[test]
    fn documentos_viram_grade() {
        let docs = vec![Ok(doc! {"_id": 1, "nome": "Ana", "end": {"cidade": "BH"}}), Ok(doc! {"_id": 2, "idade": 30}), Ok(doc! {"_id": 3})];
        let r = em_grade(docs.into_iter(), 2).unwrap();
        assert_eq!(r.colunas, ["_id", "nome", "end", "idade"]);
        assert_eq!(r.linhas[0][2].as_deref(), Some(r#"{"cidade":"BH"}"#));
        assert_eq!(r.linhas[1][1], None);
        assert!(r.cortado);
    }

    /// Contra um Mongo de verdade: `FEATHERDB_TESTE_MONGO='mongodb+srv://...' cargo test -- --ignored mongo`.
    #[test]
    #[ignore]
    fn mongo_de_verdade() {
        let url = std::env::var("FEATHERDB_TESTE_MONGO").expect("FEATHERDB_TESTE_MONGO");
        let (c, senha) = Conexao::de_url(&url).unwrap();
        let mut b = c.conectar(&senha).unwrap();
        let bancos = b.bancos().unwrap();
        println!("bancos: {bancos:?}");
        let mut b = Conexao { banco: bancos[0].clone(), ..c.clone() }.conectar(&senha).unwrap();
        let tabelas = b.tabelas().unwrap();
        println!("coleções: {:?}", tabelas.iter().map(|t| &t.nome).collect::<Vec<_>>());
        if let Some(t) = tabelas.first() {
            println!("campos: {:?}", b.colunas(t).unwrap());
            let r = b.executar(&format!("db.getCollection({:?}).find({{}}).limit(3)", t.nome), 1000).unwrap();
            println!("{:?}\n{:?}", r.colunas, r.linhas);
        }
    }
}

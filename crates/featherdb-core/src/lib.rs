//! O motor do FeatherDB: as conexões guardadas, os drivers (Postgres, MySQL, SQLite e ODBC),
//! as consultas e o catálogo (tabelas e colunas). Nada de interface aqui.
pub mod cofre;
pub mod estrutura;
pub mod formatar;
pub use estrutura::{CampoDe, Coluna};
mod mongo_banco;
mod mysql_banco;
mod odbc;
mod postgres_banco;
mod sqlite_banco;
mod tls;
mod tunel;

use std::path::{Path, PathBuf};

/// Quantas linhas uma consulta traz para a tela (o resto fica no banco).
pub const LIMITE_LINHAS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tipo {
    Postgres,
    MySql,
    Sqlite,
    /// Qualquer banco com driver ODBC instalado: SAP HANA/Datasphere, TIBCO TDV, SQL Server, Oracle…
    Odbc,
    /// MongoDB (inclusive o Atlas, `mongodb+srv://`): o endereço inteiro fica em `caminho`, sem a senha.
    MongoDb,
}

impl Tipo {
    pub const TODOS: [Tipo; 5] = [Tipo::Postgres, Tipo::MySql, Tipo::Sqlite, Tipo::MongoDb, Tipo::Odbc];
    pub fn nome(self) -> &'static str {
        match self {
            Tipo::Postgres => "PostgreSQL",
            Tipo::MySql => "MySQL / MariaDB",
            Tipo::Sqlite => "SQLite",
            Tipo::Odbc => "ODBC",
            Tipo::MongoDb => "MongoDB",
        }
    }
    pub fn porta_padrao(self) -> u16 {
        match self {
            Tipo::Postgres => 5432,
            Tipo::MySql => 3306,
            Tipo::Sqlite | Tipo::Odbc | Tipo::MongoDb => 0,
        }
    }
    fn codigo(self) -> &'static str {
        match self {
            Tipo::Postgres => "postgres",
            Tipo::MySql => "mysql",
            Tipo::Sqlite => "sqlite",
            Tipo::Odbc => "odbc",
            Tipo::MongoDb => "mongodb",
        }
    }
    fn de_codigo(c: &str) -> Option<Tipo> {
        Tipo::TODOS.into_iter().find(|t| t.codigo() == c)
    }
}

impl std::fmt::Display for Tipo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.nome())
    }
}

/// Uma conexão guardada. A senha nunca é gravada no disco: é pedida ao conectar.
#[derive(Clone, Debug, PartialEq)]
pub struct Conexao {
    pub nome: String,
    pub tipo: Tipo,
    pub host: String,
    pub porta: u16,
    pub banco: String,
    pub usuario: String,
    /// SQLite: o arquivo. ODBC: a string de conexão (`DSN=...` ou `Driver={...};Server=...`).
    pub caminho: String,
    /// Postgres e MySQL: conexão criptografada (TLS).
    pub tls: bool,
    /// Com TLS: conferir se o certificado do servidor é de quem diz ser (desligado = só criptografar,
    /// para servidores internos com certificado próprio).
    pub tls_conferir: bool,
    /// Arquivos PEM: a autoridade (CA) da empresa, e o certificado e a chave do cliente (quando o servidor pede).
    pub tls_ca: String,
    pub tls_cert: String,
    pub tls_chave: String,
    /// Guardar a senha no cofre do sistema ([`cofre`]) e não pedir de novo.
    pub lembrar_senha: bool,
    /// Só leitura: o próprio banco recusa mudanças (Postgres e MySQL pela sessão, SQLite pelo arquivo);
    /// nos outros a interface barra o que não é leitura.
    pub somente_leitura: bool,
    /// Banco de produção: cor de alerta e confirmação antes de qualquer comando que muda dados.
    pub producao: bool,
    /// Túnel SSH (Postgres e MySQL): servidor, porta, usuário e o arquivo da chave (vazio = senha).
    pub ssh_host: String,
    pub ssh_porta: u16,
    pub ssh_usuario: String,
    pub ssh_chave: String,
    /// A senha do SSH (ou da chave) nesta sessão. Nunca vai para o arquivo.
    pub ssh_senha: String,
}

impl Conexao {
    pub fn nova(tipo: Tipo) -> Conexao {
        Conexao {
            nome: String::new(),
            tipo,
            host: if matches!(tipo, Tipo::Postgres | Tipo::MySql) { "localhost".into() } else { String::new() },
            porta: tipo.porta_padrao(),
            banco: String::new(),
            usuario: String::new(),
            caminho: String::new(),
            tls: false,
            tls_conferir: true,
            tls_ca: String::new(),
            tls_cert: String::new(),
            tls_chave: String::new(),
            lembrar_senha: true,
            somente_leitura: false,
            producao: false,
            ssh_host: String::new(),
            ssh_porta: 22,
            ssh_usuario: String::new(),
            ssh_chave: String::new(),
            ssh_senha: String::new(),
        }
    }

    /// Lê um endereço como `postgresql://usuario:senha@servidor:5432/banco?sslmode=require`
    /// (também `postgres://` e `mysql://`). Devolve a conexão e a senha do endereço.
    pub fn de_url(url: &str) -> Option<(Conexao, String)> {
        let url = url.trim();
        let (esquema, resto) = url.split_once("://")?;
        if esquema == "mongodb" || esquema == "mongodb+srv" {
            // O endereço inteiro fica guardado (sem a senha): o driver do Mongo entende as opções dele.
            let fim_host = resto.find('/').unwrap_or(resto.len());
            let (credencial, depois) = match resto[..fim_host].rfind('@') {
                Some(a) => (&resto[..a], &resto[a + 1..]),
                None => ("", resto),
            };
            let (usuario, senha) = credencial.split_once(':').unwrap_or((credencial, ""));
            let senha = if senha == "<db_password>" { String::new() } else { decodificar(senha) };
            let banco = depois.split_once('/').map_or("", |(_, b)| b.split('?').next().unwrap_or(""));
            let caminho = if usuario.is_empty() { format!("{esquema}://{depois}") } else { format!("{esquema}://{usuario}@{depois}") };
            let c = Conexao { caminho, usuario: decodificar(usuario), banco: decodificar(banco), ..Conexao::nova(Tipo::MongoDb) };
            return Some((c, senha));
        }
        let tipo = match esquema {
            "postgresql" | "postgres" => Tipo::Postgres,
            "mysql" | "mariadb" => Tipo::MySql,
            _ => return None,
        };
        let (resto, consulta) = resto.split_once('?').unwrap_or((resto, ""));
        let (endereco, banco) = resto.split_once('/').unwrap_or((resto, ""));
        // A senha pode ter `@`: o servidor é o que vem depois do último.
        let (credencial, servidor) = endereco.rsplit_once('@').unwrap_or(("", endereco));
        let (usuario, senha) = credencial.split_once(':').unwrap_or((credencial, ""));
        let (host, porta) = match servidor.rsplit_once(':') {
            Some((h, p)) => (h, p.parse().ok()?),
            None => (servidor, tipo.porta_padrao()),
        };
        let tls = consulta.split('&').any(|p| matches!(p, "sslmode=require" | "sslmode=verify-full" | "sslmode=verify-ca" | "ssl=true" | "ssl-mode=REQUIRED" | "ssl-mode=VERIFY_IDENTITY" | "ssl-mode=VERIFY_CA"));
        // `require` só criptografa (como no psql); `verify-*` confere o certificado.
        let tls_conferir = !consulta.split('&').any(|p| matches!(p, "sslmode=require" | "ssl-mode=REQUIRED"));
        let c = Conexao { host: decodificar(host), porta, banco: decodificar(banco), usuario: decodificar(usuario), tls, tls_conferir, ..Conexao::nova(tipo) };
        Some((c, decodificar(senha)))
    }

    /// O que aparece na lista quando a pessoa não deu nome.
    pub fn rotulo(&self) -> String {
        if !self.nome.trim().is_empty() {
            return self.nome.clone();
        }
        match self.tipo {
            Tipo::Sqlite => Path::new(&self.caminho).file_name().map_or("SQLite".into(), |n| n.to_string_lossy().into_owned()),
            Tipo::Odbc => "ODBC".into(),
            Tipo::MongoDb => {
                // O servidor do endereço, sem esquema, usuário e opções.
                let resto = self.caminho.split_once("://").map_or(self.caminho.as_str(), |(_, r)| r);
                let resto = resto.rsplit_once('@').map_or(resto, |(_, h)| h);
                let host = resto.split(['/', '?']).next().unwrap_or("MongoDB");
                if self.banco.is_empty() { host.to_owned() } else { format!("{}@{host}", self.banco) }
            }
            _ => format!("{}@{}", if self.banco.is_empty() { self.tipo.nome() } else { &self.banco }, self.host),
        }
    }

    /// Conecta. Bloqueia: a interface chama numa thread à parte.
    /// Usa túnel SSH (só bancos com servidor e porta).
    pub fn usa_ssh(&self) -> bool {
        matches!(self.tipo, Tipo::Postgres | Tipo::MySql) && !self.ssh_host.trim().is_empty()
    }

    pub fn conectar(&self, senha: &str) -> Result<Box<dyn Banco>, String> {
        if self.usa_ssh() {
            // O banco é alcançado pela porta local do túnel. O nome no certificado TLS é o do banco,
            // não 127.0.0.1: com túnel o TLS só criptografa (o SSH já conferiu o servidor).
            let tunel = tunel::abrir(self, (self.host.clone(), self.porta))?;
            let local = Conexao { host: "127.0.0.1".into(), porta: tunel.porta, ssh_host: String::new(), tls_conferir: false, ..self.clone() };
            let banco = local.conectar(senha)?;
            return Ok(Box::new(tunel::ComTunel { banco, _tunel: tunel }));
        }
        let mut b = match self.tipo {
            Tipo::Postgres => postgres_banco::conectar(self, senha),
            Tipo::MySql => mysql_banco::conectar(self, senha),
            Tipo::Sqlite => sqlite_banco::conectar(self),
            Tipo::Odbc => odbc::conectar(&self.caminho, &self.usuario, senha),
            Tipo::MongoDb => mongo_banco::conectar(self, senha),
        }?;
        // Só leitura garantida pelo servidor, não só pela interface (o SQLite abre o arquivo só para ler).
        let travar = match (self.somente_leitura, self.tipo) {
            (true, Tipo::Postgres) => Some("SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY"),
            (true, Tipo::MySql) => Some("SET SESSION TRANSACTION READ ONLY"),
            _ => None,
        };
        if let Some(sql) = travar {
            b.executar(sql, 0)?;
        }
        Ok(b)
    }
}

impl Tipo {
    /// Bancos em que o FeatherDB abre transação (BEGIN … COMMIT/ROLLBACK) pelo modo manual.
    pub fn tem_transacao(self) -> bool {
        matches!(self, Tipo::Postgres | Tipo::MySql | Tipo::Sqlite)
    }
}

/// O que uma consulta devolveu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resultado {
    pub colunas: Vec<String>,
    /// `None` é NULL.
    pub linhas: Vec<Vec<Option<String>>>,
    /// Comandos sem linhas (INSERT, UPDATE…): quantas linhas mudaram.
    pub afetadas: Option<u64>,
    /// Havia mais que [`LIMITE_LINHAS`] linhas.
    pub cortado: bool,
    /// O tipo de cada coluna, quando o banco diz (vazio quando não diz).
    pub tipos: Vec<String>,
}

/// Uma tabela ou visão do catálogo.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tabela {
    pub esquema: String,
    pub nome: String,
    pub visao: bool,
    /// Visão materializada (Postgres): guarda o resultado e só muda com REFRESH. Também tem `visao`.
    pub materializada: bool,
}

/// Uma função ou procedimento do banco (o corpo é lido só quando a pessoa abre).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Funcao {
    pub esquema: String,
    pub nome: String,
    /// Os parâmetros, como o banco escreve (`a integer, b text`).
    pub argumentos: String,
    /// Como o banco acha esta (o oid no Postgres; o nome no MySQL).
    pub id: String,
    pub procedimento: bool,
}

impl Tabela {
    /// O nome para usar no SQL (`esquema.tabela`, com aspas quando precisa).
    pub fn qualificado(&self, tipo: Tipo) -> String {
        let q = |s: &str| identificador(s, tipo);
        if self.esquema.is_empty() { q(&self.nome) } else { format!("{}.{}", q(&self.esquema), q(&self.nome)) }
    }
}

/// Coloca aspas no identificador só quando ele não é minúsculo simples.
pub fn identificador(s: &str, tipo: Tipo) -> String {
    let simples = !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') && !s.starts_with(|c: char| c.is_ascii_digit());
    if simples {
        return s.to_owned();
    }
    match tipo {
        Tipo::MySql => format!("`{}`", s.replace('`', "``")),
        _ => format!("\"{}\"", s.replace('"', "\"\"")),
    }
}

/// Desfaz o `%40` e afins de um pedaço de endereço (senha com `@`, por exemplo).
fn decodificar(s: &str) -> String {
    let b = s.as_bytes();
    let mut saida = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                saida.push(v);
                i += 3;
                continue;
            }
        }
        saida.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&saida).into_owned()
}

/// Literal de texto SQL (aspas simples dobradas).
pub(crate) fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Cancela a consulta que está rodando, de outra thread (a conexão fica ocupada enquanto roda).
pub type Cancelar = std::sync::Arc<dyn Fn() + Send + Sync>;

/// Uma conexão aberta.
pub trait Banco: Send {
    /// Como parar a consulta em andamento; `None` quando o driver não sabe (aí a interface abandona a conexão).
    fn cancelador(&self) -> Option<Cancelar> {
        None
    }

    /// Roda o SQL e traz até `limite` linhas.
    fn executar(&mut self, sql: &str, limite: usize) -> Result<Resultado, String>;

    /// Tabelas e visões, menos as do sistema.
    fn tabelas(&mut self) -> Result<Vec<Tabela>, String>;

    /// Os bancos do servidor (Postgres e MySQL), para a árvore quando a conexão não escolheu um.
    fn bancos(&mut self) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    /// As funções e procedimentos criados pela pessoa (Postgres e MySQL; os outros não listam).
    fn funcoes(&mut self) -> Result<Vec<Funcao>, String> {
        Ok(Vec::new())
    }

    /// O SQL que cria a função, para abrir no editor e mudar.
    fn definicao(&mut self, _f: &Funcao) -> Result<String, String> {
        Err("Este banco não mostra o código das funções.".into())
    }

    /// As colunas da tabela, na ordem dela.
    fn colunas(&mut self, t: &Tabela) -> Result<Vec<Coluna>, String>;

    /// O SQL que cria a tabela. O padrão monta pelas colunas; MySQL e SQLite pedem ao próprio banco.
    fn ddl(&mut self, t: &Tabela) -> Result<String, String> {
        let colunas = self.colunas(t)?;
        Ok(estrutura::ddl(Tipo::Odbc, &t.qualificado(Tipo::Odbc), &colunas))
    }

    /// Todas as colunas de todas as tabelas (o dicionário do banco), para buscar e exportar.
    /// O padrão pergunta tabela por tabela; Postgres e MySQL respondem numa consulta só.
    fn dicionario(&mut self) -> Result<Vec<CampoDe>, String> {
        let mut saida = Vec::new();
        for t in self.tabelas()? {
            for c in self.colunas(&t)? {
                saida.push(CampoDe { esquema: t.esquema.clone(), tabela: t.nome.clone(), coluna: c.nome, tipo: c.tipo });
            }
        }
        Ok(saida)
    }
}

/// As tabelas de uma consulta ao catálogo que devolve esquema, nome e tipo (Postgres e MySQL).
pub(crate) fn tabelas_do_catalogo(b: &mut dyn Banco, sql: &str) -> Result<Vec<Tabela>, String> {
    let r = b.executar(sql, usize::MAX)?;
    Ok(r.linhas
        .into_iter()
        .map(|l| {
            let campo = |i: usize| l.get(i).cloned().flatten().unwrap_or_default();
            Tabela { esquema: campo(0), nome: campo(1), visao: campo(2).contains("VIEW"), materializada: campo(2).contains("MATERIALIZED") }
        })
        .collect())
}

/// Funções de uma consulta que devolve esquema, nome, argumentos, id e o tipo (`marca_procedimento` = procedimento).
pub(crate) fn funcoes_do_catalogo(r: Resultado, marca_procedimento: &str) -> Vec<Funcao> {
    r.linhas
        .into_iter()
        .map(|l| {
            let campo = |i: usize| l.get(i).cloned().flatten().unwrap_or_default();
            Funcao { esquema: campo(0), nome: campo(1), argumentos: campo(2), id: campo(3), procedimento: campo(4) == marca_procedimento }
        })
        .collect()
}

/// Colunas de uma consulta ao catálogo que devolve nome, tipo, aceita vazio (YES/NO), padrão e chave (S/N).
pub(crate) fn colunas_do_catalogo(b: &mut dyn Banco, sql: &str) -> Result<Vec<Coluna>, String> {
    let r = b.executar(sql, usize::MAX)?;
    Ok(r.linhas
        .into_iter()
        .map(|l| {
            let campo = |i: usize| l.get(i).cloned().flatten();
            Coluna {
                nome: campo(0).unwrap_or_default(),
                tipo: campo(1).unwrap_or_default(),
                nulo: campo(2).is_some_and(|v| v.eq_ignore_ascii_case("YES")),
                padrao: campo(3),
                chave: campo(4).as_deref() == Some("S"),
            }
        })
        .collect())
}

/// O dicionário a partir de uma consulta que devolve esquema, tabela, coluna e tipo.
pub(crate) fn dicionario_do_catalogo(b: &mut dyn Banco, sql: &str) -> Result<Vec<CampoDe>, String> {
    let r = b.executar(sql, usize::MAX)?;
    Ok(r.linhas
        .into_iter()
        .map(|l| {
            let campo = |i: usize| l.get(i).cloned().flatten().unwrap_or_default();
            CampoDe { esquema: campo(0), tabela: campo(1), coluna: campo(2), tipo: campo(3) }
        })
        .collect())
}

/// O comando em volta do cursor (`linha` e `coluna` contam de 0, coluna em caracteres): o trecho
/// entre os `;` mais próximos, ignorando `;` dentro de aspas e comentários. Vazio se só tiver espaço.
pub fn comando_no_cursor(texto: &str, linha: usize, coluna: usize) -> String {
    let mut pos = 0;
    for (i, l) in texto.split('\n').enumerate() {
        if i == linha {
            pos += l.char_indices().nth(coluna).map_or(l.len(), |(b, _)| b);
            break;
        }
        pos += l.len() + 1;
    }
    let (mut inicio, mut anterior) = (0, String::new());
    for fim in separadores(texto).into_iter().chain([texto.len()]) {
        let trecho = texto[inicio..fim].trim();
        if pos <= fim {
            // Cursor depois do `;`, num trecho só de espaço: vale o comando que acabou de fechar.
            return if trecho.is_empty() { anterior } else { trecho.to_owned() };
        }
        if !trecho.is_empty() {
            anterior = trecho.to_owned();
        }
        inicio = fim + 1;
    }
    anterior
}

/// Todos os comandos do texto, sem os vazios.
pub fn dividir(texto: &str) -> Vec<String> {
    let mut saida = Vec::new();
    let mut inicio = 0;
    for fim in separadores(texto).into_iter().chain([texto.len()]) {
        let t = texto[inicio..fim].trim();
        if !t.is_empty() {
            saida.push(t.to_owned());
        }
        inicio = (fim + 1).min(texto.len());
    }
    saida
}

/// Um `DELETE` ou `UPDATE` sem `WHERE` (mexe na tabela inteira): devolve a palavra do comando.
/// Comentários e textos entre aspas não contam (`-- where` não salva ninguém).
pub fn sem_where(sql: &str) -> Option<&'static str> {
    let b = sql.as_bytes();
    let mut limpo = String::with_capacity(sql.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            q @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                limpo.push(' ');
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            c => limpo.push(if c.is_ascii_alphanumeric() || c == b'_' { c.to_ascii_lowercase() as char } else { ' ' }),
        }
        i += 1;
    }
    let palavras: Vec<&str> = limpo.split_whitespace().collect();
    // `WITH x AS (...) DELETE ...`: vale o primeiro DELETE/UPDATE depois do WITH.
    let comando = match palavras.first() {
        Some(&"with") => palavras.iter().find(|p| matches!(**p, "delete" | "update")).copied(),
        p => p.copied(),
    }?;
    let tem_where = palavras.contains(&"where");
    match comando {
        "delete" if !tem_where => Some("DELETE"),
        "update" if !tem_where => Some("UPDATE"),
        _ => None,
    }
}

/// O erro diz que a conexão caiu (servidor reiniciou, rede piscou, ficou parada tempo demais)?
pub fn conexao_caiu(erro: &str) -> bool {
    let e = erro.to_lowercase();
    ["connection closed", "broken pipe", "connection reset", "server closed the connection", "lost connection", "gone away", "connection refused", "não conectado", "end of file"]
        .iter()
        .any(|p| e.contains(p))
}

/// Comando que só lê (SELECT, SHOW, EXPLAIN, consulta do Mongo): pode rodar de novo sem risco.
pub fn so_le(sql: &str) -> bool {
    let p = sql.trim_start().split(|c: char| !c.is_ascii_alphanumeric() && c != '.').next().unwrap_or("").to_ascii_lowercase();
    let l = sql.to_ascii_lowercase();
    let escreve = ["insert", "update", "delete", "merge", "replace", "drop", "create", "alter"].iter().any(|w| l.contains(w));
    matches!(p.as_str(), "select" | "show" | "explain" | "describe" | "desc" | "values" | "table") || ((p == "with" || p.starts_with("db.")) && !escreve)
}

/// Comando que muda dados ou estrutura (o que uma conexão só leitura barra e uma de produção confirma).
/// Leitura, SET e controle de transação não contam.
pub fn muda_dados(sql: &str) -> bool {
    let p = sql.trim_start().split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("").to_ascii_lowercase();
    !so_le(sql) && !matches!(p.as_str(), "set" | "begin" | "start" | "commit" | "rollback" | "end" | "")
}

/// Comando que muda a estrutura do banco (tabelas, views, funções): a árvore precisa ler o catálogo de novo.
pub fn muda_estrutura(sql: &str) -> bool {
    let p = sql.trim_start().split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("").to_ascii_lowercase();
    matches!(p.as_str(), "create" | "alter" | "drop" | "rename")
}

/// Posição (em bytes) de cada `;` que separa comandos.
fn separadores(texto: &str) -> Vec<usize> {
    let b = texto.as_bytes();
    let mut saida = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            q @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            // Texto entre `$$` (ou `$marca$`) do Postgres: corpo de função, com `;` dentro.
            b'$' => {
                if let Some(f) = texto[i + 1..].find('$') {
                    let marca = &texto[i..i + f + 2];
                    if marca[1..marca.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '_') {
                        if let Some(fim) = texto[i + marca.len()..].find(marca) {
                            i += marca.len() + fim + marca.len() - 1;
                        }
                    }
                }
            }
            b';' => saida.push(i),
            _ => {}
        }
        i += 1;
    }
    saida
}

/// As conexões guardadas em `conexoes.txt`: uma por linha, campos separados por tabulação.
pub fn ler_conexoes(arquivo: &Path) -> Vec<Conexao> {
    let Ok(texto) = std::fs::read_to_string(arquivo) else { return Vec::new() };
    texto
        .lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            // Os campos 9 (lembrar a senha) e 10 a 13 (certificados) vieram depois: linha antiga continua valendo.
            let (tipo, nome, host, porta, banco, usuario, caminho, tls, lembrar) = match c[..] {
                [a, b, h, p, d, u, k, t] => (a, b, h, p, d, u, k, t, "1"),
                [a, b, h, p, d, u, k, t, l, ..] => (a, b, h, p, d, u, k, t, l),
                _ => return None,
            };
            let extra = |i: usize| c.get(i).copied().unwrap_or("");
            Some(Conexao {
                nome: nome.into(),
                tipo: Tipo::de_codigo(tipo)?,
                host: host.into(),
                porta: porta.parse().ok()?,
                banco: banco.into(),
                usuario: usuario.into(),
                caminho: caminho.into(),
                tls: tls == "1",
                tls_conferir: extra(9) != "0",
                tls_ca: extra(10).into(),
                tls_cert: extra(11).into(),
                tls_chave: extra(12).into(),
                lembrar_senha: lembrar == "1",
                somente_leitura: extra(13) == "1",
                producao: extra(14) == "1",
                ssh_host: extra(15).into(),
                ssh_porta: extra(16).parse().unwrap_or(22),
                ssh_usuario: extra(17).into(),
                ssh_chave: extra(18).into(),
                ssh_senha: String::new(),
            })
        })
        .collect()
}

/// Grava num arquivo ao lado e troca de uma vez (o anterior nunca fica pela metade).
pub fn gravar_conexoes(arquivo: &Path, conexoes: &[Conexao]) -> Result<(), String> {
    let limpo = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    let texto: String = conexoes
        .iter()
        .map(|c| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                c.tipo.codigo(),
                limpo(&c.nome),
                limpo(&c.host),
                c.porta,
                limpo(&c.banco),
                limpo(&c.usuario),
                limpo(&c.caminho),
                u8::from(c.tls),
                u8::from(c.lembrar_senha),
                u8::from(c.tls_conferir),
                limpo(&c.tls_ca),
                limpo(&c.tls_cert),
                limpo(&c.tls_chave),
                u8::from(c.somente_leitura),
                u8::from(c.producao),
                limpo(&c.ssh_host),
                c.ssh_porta,
                limpo(&c.ssh_usuario),
                limpo(&c.ssh_chave)
            )
        })
        .collect();
    if let Some(pasta) = arquivo.parent() {
        std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
    }
    let tmp: PathBuf = arquivo.with_extension("tmp");
    std::fs::write(&tmp, texto).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, arquivo).map_err(|e| e.to_string())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn reconhece_conexao_caida_e_leitura() {
        assert!(conexao_caiu("connection closed"));
        assert!(conexao_caiu("MySQL server has gone away"));
        assert!(!conexao_caiu("ERROR: relation \"x\" does not exist"));
        assert!(so_le("  select 1"));
        assert!(so_le("WITH a AS (SELECT 1) SELECT * FROM a"));
        assert!(so_le("db.pedidos.find({})"));
        assert!(!so_le("update t set a = 1"));
        assert!(!so_le("WITH x AS (SELECT 1) DELETE FROM t"));
        assert!(!so_le("db.pedidos.deleteMany({})"));
        assert!(muda_dados("insert into t values (1)") && muda_dados("drop table t") && !muda_dados("select 1") && !muda_dados("COMMIT") && !muda_dados("set search_path to x"));
        assert!(muda_estrutura("CREATE VIEW v AS SELECT 1") && muda_estrutura("  drop table t") && !muda_estrutura("select 1"));
    }

    #[test]
    fn avisa_delete_e_update_sem_where() {
        assert_eq!(sem_where("DELETE FROM pedidos"), Some("DELETE"));
        assert_eq!(sem_where("update t set a = 'where' -- where"), Some("UPDATE"));
        assert_eq!(sem_where("WITH x AS (SELECT 1) DELETE FROM t"), Some("DELETE"));
        assert_eq!(sem_where("delete from t where id = 1"), None);
        assert_eq!(sem_where("UPDATE t SET a = 1\nWHERE id = 2"), None);
        assert_eq!(sem_where("SELECT * FROM t"), None);
        assert_eq!(sem_where("/* x */ delete from t"), Some("DELETE"));
    }

    #[test]
    fn divide_respeitando_aspas_e_comentarios() {
        let sql = "select 'a;b';\n-- nada; aqui\nselect 2 /* ; */;\n\n";
        assert_eq!(dividir(sql), vec!["select 'a;b'", "-- nada; aqui\nselect 2 /* ; */"]);
        let funcao = "create function f() returns int as $$ begin return 1; end $$ language plpgsql;\nselect $1;\nselect 2";
        assert_eq!(dividir(funcao).len(), 3);
        assert!(dividir(funcao)[0].ends_with("plpgsql"));
    }

    #[test]
    fn comando_do_cursor() {
        let sql = "select 1;\nselect 2;\nselect 3";
        assert_eq!(comando_no_cursor(sql, 0, 3), "select 1");
        assert_eq!(comando_no_cursor(sql, 1, 0), "select 2");
        assert_eq!(comando_no_cursor(sql, 2, 8), "select 3");
        // Logo depois do último `;`, sem nada a seguir: o comando anterior.
        assert_eq!(comando_no_cursor("select 1;\n", 1, 0), "select 1");
    }

    #[test]
    fn conexoes_ida_e_volta_sem_senha() {
        let p = std::env::temp_dir().join(format!("featherdb-conexoes-{}.txt", std::process::id()));
        let mut c = Conexao::nova(Tipo::Postgres);
        c.nome = "Produção\tnova".into();
        c.banco = "vendas".into();
        c.tls = true;
        c.tls_conferir = false;
        c.tls_ca = "/etc/ssl/empresa-ca.pem".into();
        c.somente_leitura = true;
        c.producao = true;
        c.ssh_host = "bastiao.empresa".into();
        c.ssh_porta = 2222;
        c.ssh_usuario = "ana".into();
        c.ssh_chave = "/home/ana/.ssh/id_ed25519".into();
        let d = Conexao { caminho: "Driver={HDBODBC};ServerNode=x:443".into(), ..Conexao::nova(Tipo::Odbc) };
        gravar_conexoes(&p, &[c.clone(), d.clone()]).unwrap();
        let lidas = ler_conexoes(&p);
        std::fs::remove_file(&p).unwrap();
        c.nome = "Produção nova".into();
        assert_eq!(lidas, vec![c, d]);
        // Arquivo antigo, de antes do campo "lembrar a senha".
        std::fs::write(&p, "postgres\tLoto\t10.0.0.1\t5432\tloto\tpostgres\t\t0\n").unwrap();
        let antiga = ler_conexoes(&p);
        std::fs::remove_file(&p).unwrap();
        assert_eq!((antiga.len(), antiga[0].lembrar_senha), (1, true));
    }

    #[test]
    fn endereco_postgres_e_mysql() {
        let (c, s) = Conexao::de_url("postgresql://postgres:se%40nha@192.168.0.223:5432/lotofacil_db").unwrap();
        assert_eq!((c.tipo, c.host.as_str(), c.porta, c.banco.as_str(), c.usuario.as_str(), s.as_str(), c.tls), (Tipo::Postgres, "192.168.0.223", 5432, "lotofacil_db", "postgres", "se@nha", false));
        let (c, s) = Conexao::de_url("mysql://ana@db.x.com/loja?ssl-mode=REQUIRED").unwrap();
        assert_eq!((c.tipo, c.porta, c.usuario.as_str(), s.as_str(), c.tls), (Tipo::MySql, 3306, "ana", "", true));
        assert!(Conexao::de_url("192.168.0.223").is_none());
        let (c, s) = Conexao::de_url("mongodb+srv://gm:<db_password>@cluster0.ab.mongodb.net/?retryWrites=true").unwrap();
        assert_eq!((c.tipo, c.caminho.as_str(), c.usuario.as_str(), c.banco.as_str(), s.as_str()), (Tipo::MongoDb, "mongodb+srv://gm@cluster0.ab.mongodb.net/?retryWrites=true", "gm", "", ""));
        assert_eq!(c.rotulo(), "cluster0.ab.mongodb.net");
        let (c, s) = Conexao::de_url("mongodb://ana:x%40y@localhost:27017/loja").unwrap();
        assert_eq!((c.banco.as_str(), s.as_str(), c.caminho.as_str()), ("loja", "x@y", "mongodb://ana@localhost:27017/loja"));
        assert!(Conexao::de_url("postgres://h:porta/x").is_none());
    }

    /// Contra um Postgres de verdade: `FEATHERDB_TESTE_PG=postgresql://... cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn postgres_de_verdade() {
        let url = std::env::var("FEATHERDB_TESTE_PG").expect("FEATHERDB_TESTE_PG");
        let (c, senha) = Conexao::de_url(&url).unwrap();
        let mut b = c.conectar(&senha).unwrap();
        let bancos = b.bancos().unwrap();
        println!("bancos: {bancos:?}");
        assert!(!bancos.is_empty());
        let tabelas = b.tabelas().unwrap();
        println!("{} tabelas: {:?}", tabelas.len(), tabelas.iter().take(8).map(|t| &t.nome).collect::<Vec<_>>());
        if let Some(t) = tabelas.first() {
            let cols = b.colunas(t).unwrap();
            println!("colunas de {}: {:?}", t.nome, cols);
            assert!(cols.iter().any(|c| c.chave), "a chave primária aparece");
            let d = b.dicionario().unwrap();
            println!("dicionário: {} colunas", d.len());
            assert!(d.iter().any(|c| c.tabela == t.nome));
            let r = b.executar(&format!("SELECT * FROM {}", t.qualificado(Tipo::Postgres)), 3).unwrap();
            println!("{:?}\n{:?}", r.colunas, r.linhas);
        }
        if let Some(t) = tabelas.first() {
            let ddl = b.ddl(t).unwrap();
            println!("{ddl}");
            assert!(ddl.starts_with("CREATE"));
        }
        let funcoes = b.funcoes().unwrap();
        println!("{} funções: {:?}", funcoes.len(), funcoes.iter().take(5).map(|f| format!("{}({})", f.nome, f.argumentos)).collect::<Vec<_>>());
        if let Some(f) = funcoes.first() {
            let def = b.definicao(f).unwrap();
            println!("{}", def.lines().take(3).collect::<Vec<_>>().join("\n"));
            assert!(def.starts_with("CREATE"));
        }
        // Função temporária (some ao desconectar): o código dela volta como CREATE.
        b.executar("create function pg_temp.featherdb_teste(a int) returns int language sql as $$ select a + 1 $$", 0).unwrap();
        let oid = b.executar("select 'pg_temp.featherdb_teste(int)'::regprocedure::oid::text", 1).unwrap().linhas[0][0].clone().unwrap();
        let f = Funcao { esquema: String::new(), nome: "featherdb_teste".into(), argumentos: "a integer".into(), id: oid, procedimento: false };
        assert!(b.definicao(&f).unwrap().contains("CREATE OR REPLACE FUNCTION"));
        println!("materializadas: {:?}", tabelas.iter().filter(|t| t.materializada).map(|t| &t.nome).collect::<Vec<_>>());
        // Parar: o pg_sleep de 10 s volta cancelado em vez de esperar.
        let parar = b.cancelador().unwrap();
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            parar();
        });
        let inicio = std::time::Instant::now();
        let e = b.executar("select pg_sleep(10)", 1).unwrap_err();
        t.join().unwrap();
        assert!(inicio.elapsed().as_secs() < 5 && e.contains("cancel"), "{e}");
        // Salvar células numa tabela temporária (some ao desconectar; nada do banco é mexido).
        b.executar("create temp table featherdb_teste (id int primary key, nome text)", 0).unwrap();
        b.executar("insert into featherdb_teste values (1, 'Ana'), (2, 'Bia')", 0).unwrap();
        let cols = ["id".to_owned(), "nome".to_owned()];
        let up = estrutura::sql_atualizar(Tipo::Postgres, "featherdb_teste", &cols, &[0], &[Some("2".into()), Some("Bia".into())], &[(1, Some("O'Neil".into()))]).unwrap();
        for c in ["BEGIN", up.as_str(), "COMMIT"] {
            b.executar(c, 0).unwrap();
        }
        let r = b.executar("select nome from featherdb_teste where id = 2", 1).unwrap();
        assert_eq!(r.linhas[0][0].as_deref(), Some("O'Neil"));
        let r = b.executar("select 1 as um, now()::text as agora, null as nada, 'ção' as texto", 10).unwrap();
        assert_eq!(r.colunas, ["um", "agora", "nada", "texto"]);
        assert_eq!(r.linhas[0][0].as_deref(), Some("1"));
        assert_eq!(r.linhas[0][2], None);
        assert_eq!(r.linhas[0][3].as_deref(), Some("ção"));
        assert!(b.executar("select * from tabela_que_nao_existe_featherdb", 1).unwrap_err().contains("tabela_que_nao_existe_featherdb"));
    }

    #[test]
    fn identificadores_com_aspas_so_quando_precisa() {
        assert_eq!(identificador("clientes", Tipo::Postgres), "clientes");
        assert_eq!(identificador("Clientes", Tipo::Postgres), "\"Clientes\"");
        assert_eq!(identificador("a`b", Tipo::MySql), "`a``b`");
    }

    #[test]
    fn sqlite_so_leitura_recusa_mudanca() {
        let p = std::env::temp_dir().join(format!("featherdb-leitura-{}.db", std::process::id()));
        let c = Conexao { caminho: p.to_string_lossy().into_owned(), ..Conexao::nova(Tipo::Sqlite) };
        c.conectar("").unwrap().executar("create table t (a int)", 0).unwrap();
        let mut b = Conexao { somente_leitura: true, ..c }.conectar("").unwrap();
        assert!(b.executar("select * from t", 10).is_ok());
        assert!(b.executar("insert into t values (1)", 0).unwrap_err().contains("readonly"));
        drop(b);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn sqlite_para_a_consulta_no_meio() {
        let c = Conexao { caminho: ":memory:".into(), ..Conexao::nova(Tipo::Sqlite) };
        let mut b = c.conectar("").unwrap();
        let parar = b.cancelador().unwrap();
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            parar();
        });
        let r = b.executar("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n) SELECT count(*) FROM n", 10);
        t.join().unwrap();
        assert!(r.unwrap_err().contains("interrupt"));
    }

    #[test]
    fn sqlite_de_ponta_a_ponta() {
        let p = std::env::temp_dir().join(format!("featherdb-teste-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let c = Conexao { caminho: p.to_string_lossy().into_owned(), ..Conexao::nova(Tipo::Sqlite) };
        let mut b = c.conectar("").unwrap();
        b.executar("create table pessoas (id integer primary key, nome text, nota real)", 10).unwrap();
        let r = b.executar("insert into pessoas (nome, nota) values ('Ana', 9.5), (null, 7)", 10).unwrap();
        assert_eq!(r.afetadas, Some(2));
        let r = b.executar("select id, nome, nota from pessoas order by id", 1).unwrap();
        assert_eq!(r.colunas, ["id", "nome", "nota"]);
        assert_eq!(r.linhas, vec![vec![Some("1".into()), Some("Ana".into()), Some("9.5".into())]]);
        assert!(r.cortado);
        let t = b.tabelas().unwrap();
        assert_eq!(t, vec![Tabela { nome: "pessoas".into(), ..Default::default() }]);
        let cols = b.colunas(&t[0]).unwrap();
        assert_eq!(cols[0], Coluna { nome: "id".into(), tipo: "INTEGER".into(), nulo: true, padrao: None, chave: true });
        assert_eq!((cols[1].nome.as_str(), cols[1].tipo.as_str()), ("nome", "TEXT"));
        let d = b.dicionario().unwrap();
        assert_eq!(d.len(), 3);
        assert_eq!((d[2].tabela.as_str(), d[2].coluna.as_str()), ("pessoas", "nota"));
        assert_eq!(b.ddl(&t[0]).unwrap(), "CREATE TABLE pessoas (id integer primary key, nome text, nota real);\n");
        assert!(b.executar("select * from nao_existe", 10).is_err());
        drop(b);
        std::fs::remove_file(&p).unwrap();
    }
}

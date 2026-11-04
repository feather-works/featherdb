//! ODBC: a porta para qualquer banco que tenha driver ODBC (SAP HANA/Datasphere, TIBCO TDV,
//! SQL Server, Oracle, Snowflake…). É o que o DBeaver faz com JDBC, sem precisar de Java.
//!
//! O gerenciador de drivers (unixODBC no Linux, iODBC no macOS, `odbc32.dll` no Windows) é
//! carregado só ao conectar: sem ele instalado, o FeatherDB abre normalmente e só o ODBC avisa.
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::OnceLock;

use libloading::Library;

use crate::{Banco, Resultado, Tabela};

type H = *mut c_void;
type Ret = i16;

const HANDLE_ENV: i16 = 1;
const HANDLE_DBC: i16 = 2;
const HANDLE_STMT: i16 = 3;
const ATTR_ODBC_VERSION: i32 = 200;
const OV_ODBC3: usize = 3;
const NO_DATA: Ret = 100;
const NULL_DATA: isize = -1;
const NO_TOTAL: isize = -4;
const C_WCHAR: i16 = -8;
const DRIVER_NOPROMPT: u16 = 0;

/// As funções da API ODBC que o FeatherDB usa, nas versões "W": o texto vai em UTF-16, que é o que o
/// Windows e o unixODBC usam (as versões de 8 bits seguem a página de código e estragam os acentos no Windows).
/// shortcut: o iODBC do macOS usa caracteres de 4 bytes; trocar o tipo do caractere se o FeatherDB for para o macOS.
struct Api {
    _lib: Library,
    alloc: unsafe extern "system" fn(i16, H, *mut H) -> Ret,
    free: unsafe extern "system" fn(i16, H) -> Ret,
    set_env_attr: unsafe extern "system" fn(H, i32, H, i32) -> Ret,
    driver_connect: unsafe extern "system" fn(H, H, *const u16, i16, *mut u16, i16, *mut i16, u16) -> Ret,
    disconnect: unsafe extern "system" fn(H) -> Ret,
    exec_direct: unsafe extern "system" fn(H, *const u16, i32) -> Ret,
    num_result_cols: unsafe extern "system" fn(H, *mut i16) -> Ret,
    describe_col: unsafe extern "system" fn(H, u16, *mut u16, i16, *mut i16, *mut i16, *mut usize, *mut i16, *mut i16) -> Ret,
    fetch: unsafe extern "system" fn(H) -> Ret,
    get_data: unsafe extern "system" fn(H, u16, i16, H, isize, *mut isize) -> Ret,
    row_count: unsafe extern "system" fn(H, *mut isize) -> Ret,
    tables: unsafe extern "system" fn(H, *const u16, i16, *const u16, i16, *const u16, i16, *const u16, i16) -> Ret,
    columns: unsafe extern "system" fn(H, *const u16, i16, *const u16, i16, *const u16, i16, *const u16, i16) -> Ret,
    diag: unsafe extern "system" fn(i16, H, i16, *mut u16, *mut i32, *mut u16, i16, *mut i16) -> Ret,
}

const BIBLIOTECAS: &[&str] = if cfg!(windows) {
    &["odbc32.dll"]
} else if cfg!(target_os = "macos") {
    &["libiodbc.2.dylib", "libodbc.2.dylib"]
} else {
    &["libodbc.so.2", "libodbc.so"]
};

fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    API.get_or_init(|| {
        // SAFETY: é a biblioteca do gerenciador ODBC do sistema; os tipos seguem o sql.h (SQLLEN = isize).
        unsafe {
            let lib = BIBLIOTECAS
                .iter()
                .find_map(|n| Library::new(n).ok())
                .ok_or_else(|| "O gerenciador ODBC não está instalado (no Linux: o pacote unixodbc).".to_string())?;
            macro_rules! f {
                ($nome:literal) => {
                    *lib.get($nome).map_err(|e| e.to_string())?
                };
            }
            Ok(Api {
                alloc: f!(b"SQLAllocHandle"),
                free: f!(b"SQLFreeHandle"),
                set_env_attr: f!(b"SQLSetEnvAttr"),
                driver_connect: f!(b"SQLDriverConnectW"),
                disconnect: f!(b"SQLDisconnect"),
                exec_direct: f!(b"SQLExecDirectW"),
                num_result_cols: f!(b"SQLNumResultCols"),
                describe_col: f!(b"SQLDescribeColW"),
                fetch: f!(b"SQLFetch"),
                get_data: f!(b"SQLGetData"),
                row_count: f!(b"SQLRowCount"),
                tables: f!(b"SQLTablesW"),
                columns: f!(b"SQLColumnsW"),
                diag: f!(b"SQLGetDiagRecW"),
                _lib: lib,
            })
        }
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Texto em UTF-16 (sem o zero final: os tamanhos vão explícitos).
fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn de_w(u: &[u16]) -> String {
    String::from_utf16_lossy(u)
}

fn ok(r: Ret) -> bool {
    r == 0 || r == 1
}

/// As mensagens de erro do driver para o handle.
fn diagnostico(api: &Api, tipo: i16, h: H) -> String {
    let mut msgs = Vec::new();
    for rec in 1..=5 {
        let (mut estado, mut nativo, mut msg, mut tam) = ([0u16; 6], 0i32, [0u16; 1024], 0i16);
        // SAFETY: buffers com o tamanho informado.
        let r = unsafe { (api.diag)(tipo, h, rec, estado.as_mut_ptr(), &mut nativo, msg.as_mut_ptr(), msg.len() as i16, &mut tam) };
        if !ok(r) {
            break;
        }
        let n = (tam.max(0) as usize).min(msg.len() - 1);
        msgs.push(format!("[{}] {}", de_w(&estado[..5]), de_w(&msg[..n])));
    }
    if msgs.is_empty() { "erro ODBC sem mensagem do driver".into() } else { msgs.join("\n") }
}

struct Odbc {
    api: &'static Api,
    env: H,
    dbc: H,
}

// SAFETY: a conexão é usada por uma thread de cada vez (a interface guarda num Mutex); os handles
// ODBC podem trocar de thread entre chamadas.
unsafe impl Send for Odbc {}

pub(crate) fn conectar(texto: &str, usuario: &str, senha: &str) -> Result<Box<dyn Banco>, String> {
    let api = api()?;
    if texto.trim().is_empty() {
        return Err("Escreva a string de conexão ODBC (DSN=... ou Driver={...};...).".into());
    }
    let mut conexao = texto.trim().trim_end_matches(';').to_owned();
    if !usuario.is_empty() {
        conexao.push_str(&format!(";UID={usuario}"));
    }
    if !senha.is_empty() {
        conexao.push_str(&format!(";PWD={{{}}}", senha.replace('}', "}}")));
    }
    // SAFETY: sequência padrão do ODBC 3; cada handle é liberado no Drop ou no erro.
    unsafe {
        let mut env = null_mut();
        if !ok((api.alloc)(HANDLE_ENV, null_mut(), &mut env)) {
            return Err("O ODBC não abriu o ambiente.".into());
        }
        (api.set_env_attr)(env, ATTR_ODBC_VERSION, OV_ODBC3 as H, 0);
        let mut dbc = null_mut();
        if !ok((api.alloc)(HANDLE_DBC, env, &mut dbc)) {
            (api.free)(HANDLE_ENV, env);
            return Err("O ODBC não abriu a conexão.".into());
        }
        let conexao = w(&conexao);
        let mut saida = [0u16; 1024];
        let mut tam = 0i16;
        let r = (api.driver_connect)(dbc, null_mut(), conexao.as_ptr(), conexao.len() as i16, saida.as_mut_ptr(), saida.len() as i16, &mut tam, DRIVER_NOPROMPT);
        if !ok(r) {
            let e = diagnostico(api, HANDLE_DBC, dbc);
            (api.free)(HANDLE_DBC, dbc);
            (api.free)(HANDLE_ENV, env);
            return Err(e);
        }
        Ok(Box::new(Odbc { api, env, dbc }))
    }
}

impl Drop for Odbc {
    fn drop(&mut self) {
        // SAFETY: handles criados em `conectar`, liberados uma vez.
        unsafe {
            (self.api.disconnect)(self.dbc);
            (self.api.free)(HANDLE_DBC, self.dbc);
            (self.api.free)(HANDLE_ENV, self.env);
        }
    }
}

/// Um comando ODBC, liberado no fim do escopo.
struct Comando<'a>(&'a Api, H);

impl Drop for Comando<'_> {
    fn drop(&mut self) {
        // SAFETY: handle do comando, liberado uma vez.
        unsafe { (self.0.free)(HANDLE_STMT, self.1) };
    }
}

impl Odbc {
    fn comando(&self) -> Result<Comando<'_>, String> {
        let mut st = null_mut();
        // SAFETY: dbc conectado.
        if !ok(unsafe { (self.api.alloc)(HANDLE_STMT, self.dbc, &mut st) }) {
            return Err(diagnostico(self.api, HANDLE_DBC, self.dbc));
        }
        Ok(Comando(self.api, st))
    }

    /// Lê as linhas do comando já executado, como texto.
    fn ler(&self, st: &Comando<'_>, limite: usize) -> Result<Resultado, String> {
        let (api, h) = (self.api, st.1);
        let mut ncol = 0i16;
        // SAFETY: comando executado; buffers com tamanho informado.
        unsafe {
            (api.num_result_cols)(h, &mut ncol);
            if ncol == 0 {
                let mut n = 0isize;
                (api.row_count)(h, &mut n);
                return Ok(Resultado { afetadas: Some(n.max(0) as u64), ..Resultado::default() });
            }
            let mut r = Resultado::default();
            for i in 1..=ncol as u16 {
                let (mut nome, mut tam, mut tipo, mut tamcol, mut dec, mut nulo) = ([0u16; 256], 0i16, 0i16, 0usize, 0i16, 0i16);
                (api.describe_col)(h, i, nome.as_mut_ptr(), nome.len() as i16, &mut tam, &mut tipo, &mut tamcol, &mut dec, &mut nulo);
                let n = (tam.max(0) as usize).min(nome.len() - 1);
                r.colunas.push(de_w(&nome[..n]));
            }
            loop {
                let f = (api.fetch)(h);
                if f == NO_DATA {
                    break;
                }
                if !ok(f) {
                    return Err(diagnostico(api, HANDLE_STMT, h));
                }
                if r.linhas.len() >= limite {
                    r.cortado = true;
                    break;
                }
                let mut linha = Vec::with_capacity(ncol as usize);
                for i in 1..=ncol as u16 {
                    linha.push(self.valor(h, i)?);
                }
                r.linhas.push(linha);
            }
            Ok(r)
        }
    }

    /// Um valor como texto, em pedaços (textos longos vêm em várias chamadas).
    unsafe fn valor(&self, h: H, coluna: u16) -> Result<Option<String>, String> {
        let mut texto = Vec::new();
        let mut buf = [0u16; 2048];
        let bytes = (buf.len() * 2) as isize;
        loop {
            let mut ind = 0isize;
            let r = (self.api.get_data)(h, coluna, C_WCHAR, buf.as_mut_ptr() as H, bytes, &mut ind);
            if r == NO_DATA {
                break;
            }
            if !ok(r) {
                return Err(diagnostico(self.api, HANDLE_STMT, h));
            }
            if ind == NULL_DATA {
                return Ok(None);
            }
            // `ind` conta bytes. Cabe no buffer: veio tudo. Senão veio o buffer menos o zero final.
            let veio = if ind == NO_TOTAL || ind >= bytes - 1 { buf.len() - 1 } else { ind as usize / 2 };
            texto.extend_from_slice(&buf[..veio]);
            if r == 0 {
                break;
            }
        }
        Ok(Some(de_w(&texto)))
    }
}

impl Banco for Odbc {
    fn executar(&mut self, sql: &str, limite: usize) -> Result<Resultado, String> {
        let st = self.comando()?;
        let sql = w(sql);
        // SAFETY: texto com tamanho explícito.
        let r = unsafe { (self.api.exec_direct)(st.1, sql.as_ptr(), sql.len() as i32) };
        if r != NO_DATA && !ok(r) {
            return Err(diagnostico(self.api, HANDLE_STMT, st.1));
        }
        self.ler(&st, limite)
    }

    fn tabelas(&mut self) -> Result<Vec<Tabela>, String> {
        let st = self.comando()?;
        let tipos = w("TABLE,VIEW");
        // SAFETY: nulos com tamanho 0 = "todos"; tipos com tamanho explícito.
        let r = unsafe { (self.api.tables)(st.1, std::ptr::null(), 0, std::ptr::null(), 0, std::ptr::null(), 0, tipos.as_ptr(), tipos.len() as i16) };
        if !ok(r) {
            return Err(diagnostico(self.api, HANDLE_STMT, st.1));
        }
        // Colunas do SQLTables: TABLE_CAT, TABLE_SCHEM, TABLE_NAME, TABLE_TYPE.
        let res = self.ler(&st, usize::MAX)?;
        Ok(res
            .linhas
            .into_iter()
            .map(|l| {
                let c = |i: usize| l.get(i).cloned().flatten().unwrap_or_default();
                Tabela { esquema: c(1), nome: c(2), visao: c(3).contains("VIEW") }
            })
            .collect())
    }

    fn colunas(&mut self, t: &Tabela) -> Result<Vec<(String, String)>, String> {
        let st = self.comando()?;
        let (esq, nome) = (w(&t.esquema), w(&t.nome));
        let (pe, te) = if esq.is_empty() { (std::ptr::null(), 0) } else { (esq.as_ptr(), esq.len() as i16) };
        // SAFETY: textos com tamanho explícito.
        let r = unsafe { (self.api.columns)(st.1, std::ptr::null(), 0, pe, te, nome.as_ptr(), nome.len() as i16, std::ptr::null(), 0) };
        if !ok(r) {
            return Err(diagnostico(self.api, HANDLE_STMT, st.1));
        }
        // Colunas do SQLColumns: 4 = COLUMN_NAME, 6 = TYPE_NAME.
        let res = self.ler(&st, usize::MAX)?;
        Ok(res.linhas.into_iter().map(|l| (l.get(3).cloned().flatten().unwrap_or_default(), l.get(5).cloned().flatten().unwrap_or_default())).collect())
    }
}

#[cfg(test)]
mod testes {
    #[test]
    fn sem_texto_de_conexao_avisa_sem_quebrar() {
        // Com ou sem unixODBC instalado, nunca entra em pânico.
        assert!(super::conectar("", "", "").is_err());
    }

    #[test]
    fn dsn_inexistente_traz_o_erro_do_gerenciador() {
        if super::api().is_err() {
            return; // sem unixODBC nesta máquina
        }
        let e = super::conectar("DSN=featherdb_nao_existe", "u", "p").err().unwrap();
        assert!(e.contains("IM002"), "{e}");
    }
}

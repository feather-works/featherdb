//! FeatherDB: cliente de banco de dados leve. À esquerda as conexões com as tabelas; à direita o
//! editor de SQL e a grade do resultado. Postgres, MySQL, SQLite e qualquer banco com ODBC.
mod ajuda;
mod auxiliares;
mod celulas;
mod completar;
mod config;
mod estrutura_ui;
mod formulario;
mod grade;
mod historico;
mod idioma;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use feather_ui::cromo::{self, AcoesJanela};

use feather_ui::tema::*;
use featherdb_core::{Banco, CampoDe, Coluna, Conexao, Resultado, Tabela, Tipo, LIMITE_LINHAS};
use pelicano::{
    button, column, container, horizontal_space, mouse_area, row, svg, text, text_editor, tooltip, Align, Border, Content,
    Element, Event, Font, Key, Length, Modifiers, NamedKey, ResizeDirection, Settings, Space, Style, Svg, SvgView, Task, Theme,
    TooltipPosition, WindowCmd,
};

use auxiliares::{Aux, Janela};
use estrutura_ui::{Aba, CampoColuna, EdicaoColuna};
use formulario::{Campo, Formulario};
use idioma::{tr, trf};

/// Cor da marca do FeatherDB (pena azul-petróleo).
const COR_MARCA: &str = "#0b7285";
const LARGURA_LATERAL: f32 = 270.0;
const MAX_ARVORE: usize = 500;

type Aberto = Arc<Mutex<Box<dyn Banco>>>;

/// Onde um comando vai: a conexão e, quando ela lista os bancos do servidor, o banco escolhido.
type Alvo = (u64, Option<String>);

/// Um banco aberto na árvore: a conexão com ele e o que já foi lido do catálogo.
#[derive(Default)]
struct Base {
    banco: Option<Aberto>,
    conectando: bool,
    expandido: bool,
    tabelas: Vec<Tabela>,
    /// Colunas das tabelas já lidas, pelo nome qualificado.
    colunas: HashMap<String, Vec<Coluna>>,
    tabelas_abertas: HashSet<String>,
    /// Todas as colunas do banco, para a busca e para exportar (lido na primeira busca).
    dicionario: Option<Vec<CampoDe>>,
    lendo_dicionario: bool,
}

/// Uma conexão da lista, com o que está aberto dela nesta sessão.
struct Item {
    id: u64,
    conexao: Conexao,
    /// A senha desta sessão (digitada ou lida do cofre do sistema; nunca vai para arquivo).
    senha: Option<String>,
    /// A conexão principal (com o banco escolhido, ou com o servidor quando nenhum foi escolhido).
    principal: Base,
    /// Conexão sem banco escolhido (Postgres/MySQL): os bancos do servidor, cada um com a sua conexão.
    bancos: Option<Vec<String>>,
    bases: HashMap<String, Base>,
}

/// Onde o botão direito foi apertado.
#[derive(Clone)]
enum Menu {
    Conexao(u64),
    Banco(u64, String),
    Tabela(Alvo, Tabela),
}

#[derive(Clone)]
enum Msg {
    Janela(WindowCmd),
    Puxar(ResizeDirection),
    Maximizada(bool),
    Fechar,
    Aux(Aux),
    // Conexões
    Nova,
    Editar(u64),
    Excluir(u64),
    ConfirmarExclusao(bool),
    ConfirmarPerigo(bool),
    Filtro(String),
    Form(Campo),
    Senha(String),
    LembrarSenha(bool),
    ConfirmarSenha(bool),
    Conectado(Alvo, Result<Aberto, String>),
    Desconectar(u64),
    Expandir(u64),
    ExpandirBanco(u64, String),
    Bancos(u64, Result<Vec<String>, String>),
    Atualizar(Alvo),
    Tabelas(Alvo, Result<Vec<Tabela>, String>),
    ExpandirTabela(Alvo, Tabela),
    /// O último campo diz se é para exportar depois de ler.
    Colunas(Alvo, String, Result<Vec<Coluna>, String>, bool),
    VerEstrutura(Alvo, Tabela),
    Aba(Aba),
    Ddl(String, Result<String, String>),
    CopiarDdl,
    DdlNoEditor,
    Formatar,
    Sugestao(completar::Tecla),
    Celula(celulas::AcaoCelula),
    Explicar,
    EditarColuna(Option<Coluna>),
    Edicao(CampoColuna),
    CopiarColunas,
    ExportarColunas,
    ExportarColunasDe(Alvo, Tabela),
    ExportarDicionario(Alvo),
    Dicionario(Alvo, Result<Vec<CampoDe>, String>, bool),
    AbrirTabela(Alvo, Tabela),
    // Menu do botão direito
    AbrirMenu(Menu, pelicano::Point),
    FecharMenu,
    DoMenu(Box<Msg>),
    Conectar(u64),
    EscreverSelect(Alvo, Tabela),
    Contar(Alvo, Tabela),
    CopiarTexto(String),
    TamanhoJanela(pelicano::Size),
    // Consultas
    EditorMudou,
    Executar,
    ExecutarTudo,
    /// O número da rodada (resposta de rodada abandonada pelo Parar é ignorada), o resultado, o tempo
    /// e se a conexão tinha caído e foi refeita.
    Executado(u64, Result<Resultado, String>, Duration, bool),
    Parar,
    Historico(Option<String>),
    UsarHistorico(String),
    Rolou(f32, f32, f32, f32),
    Ordenar(usize),
    FiltroGrade(String),
    Copiar,
    ExportarCsv,
    Exportado(Option<Result<std::path::PathBuf, String>>),
}

struct App {
    itens: Vec<Item>,
    proximo_id: u64,
    ativa: Option<Alvo>,
    editor: Content,
    resultado: Option<Resultado>,
    erro: Option<String>,
    executando: bool,
    tempo: Option<Duration>,
    /// Posição e tamanho da rolagem da grade (ela monta só o que aparece).
    rolagem: (f32, f32),
    vista: (f32, f32),
    /// Sobe a cada resultado novo: a rolagem da grade volta ao começo.
    versao_resultado: u64,
    /// Ordem da grade: a coluna e se é crescente; o filtro; e as linhas que sobram, na ordem da tela.
    ordenacao: Option<(usize, bool)>,
    filtro_grade: String,
    visiveis: Vec<usize>,
    /// A tabela aberta no painel de baixo (abas Dados e Estrutura).
    tabela_aberta: Option<(Alvo, Tabela)>,
    aba: Aba,
    /// O CREATE da tabela aberta (aba SQL): de qual tabela e o que veio (`None` enquanto lê).
    ddl: Option<(String, Option<Result<String, String>>)>,
    /// A lista de nomes para completar no editor, quando aberta.
    sugestao: Option<completar::Sugestao>,
    /// As células editadas na grade, ainda não gravadas.
    celulas: celulas::Celulas,
    edicao: Option<EdicaoColuna>,
    /// Depois de um ALTER TABLE: ler de novo as colunas desta tabela.
    recarregar: Option<(Alvo, Tabela)>,
    aviso: String,
    form: Option<Formulario>,
    /// Pedindo a senha para conectar: a conexão e o que já foi digitado.
    pedir_senha: Option<(u64, String)>,
    excluir: Option<u64>,
    /// Comandos esperando confirmação (DELETE/UPDATE sem WHERE) e qual dos dois é.
    perigo: Option<(Vec<String>, &'static str)>,
    /// Sobe a cada consulta: a resposta de uma consulta abandonada não sobrescreve a atual.
    rodada: u64,
    cancelar: Option<featherdb_core::Cancelar>,
    /// A janela do histórico aberta, com o texto da busca.
    historico: Option<(String, Vec<historico::Entrada>)>,
    /// O texto do filtro da árvore de tabelas.
    filtro: String,
    /// O menu do botão direito aberto, e onde.
    menu: Option<(Menu, pelicano::Point)>,
    /// Tamanho da janela, para o menu não sair dela.
    tamanho: pelicano::Size,
    maximizada: bool,
    // Janelas auxiliares (auxiliares.rs)
    config: config::Config,
    janelas: feather_ui::janelas::Janelas<Janela>,
    janela_alternada: Option<Instant>,
    ultimo_clique_titulo: Option<Instant>,
    ajuda: feather_ui::ajuda::Estado,
    secao_config: usize,
    versao_escolhida: usize,
}

/// Roda `f` numa thread do tokio que pode bloquear (driver de banco) e manda o resultado.
fn em_segundo_plano<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static, msg: impl FnOnce(T) -> Msg + Send + 'static) -> Task<Msg> {
    Task::perform(async move { tokio::task::spawn_blocking(f).await.expect("tarefa de banco") }, msg)
}

fn arquivo_conexoes() -> Option<std::path::PathBuf> {
    config::pasta().map(|p| p.join("conexoes.txt"))
}

fn arquivo_rascunho() -> Option<std::path::PathBuf> {
    config::pasta().map(|p| p.join("rascunho.sql"))
}

/// Botão de ação da barra com o ícone de traço monocromático (sem as cores da faixa).
fn acao<'a>(nome: &str, dica: &str, msg: Option<Msg>) -> Element<'a, Msg> {
    cromo::acao_icone(feather_ui::icones::icone_traco(nome, 18.0), dica, msg)
}

/// Traço Tabler para ícones que faltam no catálogo da feather-ui.
fn icone_tabler(caminho: &str, tamanho: f32) -> SvgView {
    let s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"#000\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"{caminho}\"/></svg>");
    svg(Svg::from_bytes(s.as_bytes()).expect("ícone válido")).color(COR_TEXTO()).width(tamanho).height(tamanho)
}
const VISAO: &str = "M10 12a2 2 0 1 0 4 0a2 2 0 0 0 -4 0M21 12c-2.4 4 -5.4 6 -9 6c-3.6 0 -6.6 -2 -9 -6c2.4 -4 5.4 -6 9 -6c3.6 0 6.6 2 9 6";
const SETA_DIREITA: &str = "M9 6l6 6l-6 6";
const SETA_BAIXO: &str = "M6 9l6 6l6 -6";
const BANCO: &str = "M12 6m-8 0a8 3 0 1 0 16 0a8 3 0 1 0 -16 0M4 6v6a8 3 0 0 0 16 0v-6M4 12v6a8 3 0 0 0 16 0v-6";
const DESCONECTAR: &str = "M20 16l-4 4M7 12l5 5l-1.5 1.5a3.536 3.536 0 1 1 -5 -5l1.5 -1.5zM17 12l-5 -5l1.5 -1.5a3.536 3.536 0 1 1 5 5l-1.5 1.5zM3 21l2.5 -2.5M18.5 5.5l2.5 -2.5M10 11l-2 2M13 14l-2 2M16 16l4 4";
const EXECUTAR_TUDO: &str = "M7 4v16l13 -8zM3 4v16";

/// A pena da suíte na cor do FeatherDB.
/// shortcut: a marca é recolorida aqui; passar para `feather_ui::marca::Produto` quando a feather-ui ganhar o FeatherDB.
fn marca(tamanho: f32) -> SvgView {
    let s = feather_ui::marca::PENA_ORIGINAL.replace("#287846", COR_MARCA).replace("#72aa86", "#66a8b5");
    svg(Svg::from_bytes(s.as_bytes()).expect("marca válida")).width(tamanho).height(tamanho)
}

impl App {
    fn novo(config: config::Config) -> App {
        let conexoes = arquivo_conexoes().map(|a| featherdb_core::ler_conexoes(&a)).unwrap_or_default();
        let rascunho = arquivo_rascunho().and_then(|a| std::fs::read_to_string(a).ok()).unwrap_or_default();
        let itens: Vec<Item> = conexoes.into_iter().enumerate().map(|(i, c)| Item::novo(i as u64 + 1, c)).collect();
        App {
            proximo_id: itens.len() as u64 + 1,
            ativa: itens.first().map(|i| (i.id, None)),
            itens,
            editor: Content::with_text(&rascunho),
            resultado: None,
            erro: None,
            executando: false,
            tempo: None,
            rolagem: (0.0, 0.0),
            vista: (900.0, 400.0),
            versao_resultado: 0,
            ordenacao: None,
            filtro_grade: String::new(),
            visiveis: Vec::new(),
            tabela_aberta: None,
            aba: Aba::Dados,
            ddl: None,
            sugestao: None,
            celulas: Default::default(),
            edicao: None,
            recarregar: None,
            aviso: String::new(),
            form: None,
            pedir_senha: None,
            excluir: None,
            perigo: None,
            rodada: 0,
            cancelar: None,
            historico: None,
            filtro: String::new(),
            menu: None,
            tamanho: pelicano::Size::new(1280.0, 800.0),
            maximizada: false,
            config,
            janelas: Default::default(),
            janela_alternada: None,
            ultimo_clique_titulo: None,
            ajuda: Default::default(),
            secao_config: 0,
            versao_escolhida: 0,
        }
    }

    fn item(&mut self, id: u64) -> Option<&mut Item> {
        self.itens.iter_mut().find(|i| i.id == id)
    }

    fn gravar_conexoes(&mut self) {
        let lista: Vec<Conexao> = self.itens.iter().map(|i| i.conexao.clone()).collect();
        if let Some(a) = arquivo_conexoes() {
            if let Err(e) = featherdb_core::gravar_conexoes(&a, &lista) {
                self.aviso = trf("Não foi possível guardar as conexões: {}", &[&e]);
            }
        }
    }

    /// Guarda o texto do editor a cada mudança, para nada se perder se o app ou o computador cair.
    fn gravar_rascunho(&self) {
        if let Some(a) = arquivo_rascunho() {
            if let Some(p) = a.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let tmp = a.with_extension("tmp");
            if std::fs::write(&tmp, self.editor.text()).is_ok() {
                let _ = std::fs::rename(tmp, a);
            }
        }
    }

    /// O que o Ctrl+Enter roda: a seleção, ou o comando em volta do cursor.
    fn comando_atual(&self) -> String {
        match self.editor.selection().filter(|s| !s.trim().is_empty()) {
            Some(s) => s,
            None => {
                // O pelicano conta linha e coluna a partir de 1; o motor, de 0.
                let (l, c) = self.editor.cursor();
                featherdb_core::comando_no_cursor(&self.editor.text(), l.saturating_sub(1), c.saturating_sub(1))
            }
        }
    }

    /// A senha é pedida para Postgres e MySQL, e no ODBC quando há usuário.
    fn precisa_senha(c: &Conexao) -> bool {
        match c.tipo {
            Tipo::Postgres | Tipo::MySql => true,
            Tipo::Odbc | Tipo::MongoDb => !c.usuario.is_empty(),
            Tipo::Sqlite => false,
        }
    }

    fn conectar(&mut self, id: u64) -> Task<Msg> {
        let Some(item) = self.item(id) else { return Task::none() };
        if item.principal.banco.is_some() || item.principal.conectando {
            return Task::none();
        }
        if item.senha.is_none() && App::precisa_senha(&item.conexao) {
            // Senha lembrada no cofre do sistema: conecta sem perguntar.
            item.senha = item.conexao.lembrar_senha.then(|| featherdb_core::cofre::ler(&item.conexao)).flatten();
            if item.senha.is_none() {
                self.pedir_senha = Some((id, String::new()));
                return Task::none();
            }
        }
        item.principal.conectando = true;
        let (c, senha) = (item.conexao.clone(), item.senha.clone().unwrap_or_default());
        self.ativa = Some((id, None));
        em_segundo_plano(move || c.conectar(&senha).map(|b| Arc::new(Mutex::new(b))), move |r| Msg::Conectado((id, None), r))
    }

    /// Abre um banco do servidor (no Postgres cada banco é uma conexão), com a mesma senha.
    fn abrir_banco(&mut self, id: u64, nome: String) -> Task<Msg> {
        let Some(item) = self.item(id) else { return Task::none() };
        let c = Conexao { banco: nome.clone(), ..item.conexao.clone() };
        let senha = item.senha.clone().unwrap_or_default();
        let base = item.bases.entry(nome.clone()).or_default();
        if base.banco.is_some() || base.conectando {
            return Task::none();
        }
        base.conectando = true;
        em_segundo_plano(move || c.conectar(&senha).map(|b| Arc::new(Mutex::new(b))), move |r| Msg::Conectado((id, Some(nome)), r))
    }

    /// Refaz a ordem da grade (depois de ordenar, filtrar ou chegar resultado novo).
    fn reordenar(&mut self) {
        self.visiveis = self.resultado.as_ref().map(|r| grade::ordem(r, self.ordenacao, &self.filtro_grade)).unwrap_or_default();
    }

    fn base(&self, alvo: &Alvo) -> Option<&Base> {
        let item = self.itens.iter().find(|i| i.id == alvo.0)?;
        match &alvo.1 {
            None => Some(&item.principal),
            Some(n) => item.bases.get(n),
        }
    }

    fn base_mut(&mut self, alvo: &Alvo) -> Option<&mut Base> {
        let item = self.itens.iter_mut().find(|i| i.id == alvo.0)?;
        match &alvo.1 {
            None => Some(&mut item.principal),
            Some(n) => item.bases.get_mut(n),
        }
    }

    fn tipo(&self, id: u64) -> Tipo {
        self.itens.iter().find(|i| i.id == id).map_or(Tipo::Postgres, |i| i.conexao.tipo)
    }

    fn carregar_tabelas(&self, alvo: Alvo) -> Task<Msg> {
        let Some(banco) = self.base(&alvo).and_then(|b| b.banco.clone()) else { return Task::none() };
        em_segundo_plano(move || banco.lock().map_err(|_| tr("A conexão travou; desconecte e conecte de novo.").to_owned())?.tabelas(), move |r| Msg::Tabelas(alvo, r))
    }

    fn carregar_bancos(&self, id: u64) -> Task<Msg> {
        let Some(banco) = self.base(&(id, None)).and_then(|b| b.banco.clone()) else { return Task::none() };
        em_segundo_plano(move || banco.lock().map_err(|_| tr("A conexão travou; desconecte e conecte de novo.").to_owned())?.bancos(), move |r| Msg::Bancos(id, r))
    }

    /// Roda os comandos na conexão ativa; fica o resultado do último. DELETE ou UPDATE sem WHERE
    /// pede confirmação antes (um esquecimento apagaria a tabela inteira).
    fn rodar(&mut self, comandos: Vec<String>) -> Task<Msg> {
        if let Some(c) = comandos.iter().find_map(|c| featherdb_core::sem_where(c)) {
            if !self.executando {
                self.perigo = Some((comandos, c));
            }
            return Task::none();
        }
        self.rodar_conferido(comandos)
    }

    fn rodar_conferido(&mut self, comandos: Vec<String>) -> Task<Msg> {
        if comandos.is_empty() || self.executando {
            return Task::none();
        }
        let Some(alvo) = self.ativa.clone().filter(|(id, _)| self.itens.iter().any(|i| i.id == *id)) else {
            self.erro = Some(tr("Escolha uma conexão à esquerda (ou crie uma em Nova conexão).").into());
            return Task::none();
        };
        let Some(banco) = self.base(&alvo).and_then(|b| b.banco.clone()) else {
            self.erro = Some(tr("Conecte antes de rodar: clique na conexão à esquerda.").into());
            return match alvo.1 {
                None => self.conectar(alvo.0),
                Some(n) => self.abrir_banco(alvo.0, n),
            };
        };
        self.executando = true;
        self.erro = None;
        self.gravar_rascunho();
        self.rodada += 1;
        let rodada = self.rodada;
        // O cancelador sai antes de a consulta pegar a conexão (enquanto roda, ela fica ocupada).
        self.cancelar = banco.try_lock().ok().and_then(|b| b.cancelador());
        let item = self.itens.iter().find(|i| i.id == alvo.0);
        let rotulo = item.map(|i| i.conexao.rotulo()).unwrap_or_default();
        historico::anotar(&comandos, &match &alvo.1 {
            Some(n) => format!("{rotulo} / {n}"),
            None => rotulo,
        });
        let conexao = item.map(|i| Conexao { banco: alvo.1.clone().unwrap_or_else(|| i.conexao.banco.clone()), ..i.conexao.clone() });
        let senha = item.and_then(|i| i.senha.clone()).unwrap_or_default();
        em_segundo_plano(
            move || {
                let inicio = Instant::now();
                let mut b = match banco.lock() {
                    Ok(b) => b,
                    Err(_) => return (Err(tr("A conexão travou; desconecte e conecte de novo.").to_owned()), inicio.elapsed(), false),
                };
                let mut r = Ok(Resultado::default());
                let mut nova = false;
                for (i, c) in comandos.iter().enumerate() {
                    r = b.executar(c, LIMITE_LINHAS);
                    // A conexão caiu (servidor reiniciou, ficou parada tempo demais): conecta de novo uma vez.
                    // Leitura roda de novo; comando que muda dados não, porque pode ter chegado a rodar.
                    if let (Err(e), Some(con), false) = (&r, &conexao, nova) {
                        if featherdb_core::conexao_caiu(e) {
                            if let Ok(novo) = con.conectar(&senha) {
                                *b = novo;
                                nova = true;
                                r = if featherdb_core::so_le(c) && i == 0 {
                                    b.executar(c, LIMITE_LINHAS)
                                } else {
                                    Err(tr("A conexão tinha caído e foi refeita. O comando não rodou: confira e rode de novo.").to_owned())
                                };
                            }
                        }
                    }
                    if r.is_err() {
                        break;
                    }
                }
                // Gravação em bloco (BEGIN … COMMIT) que falhou no meio: desfaz o que já tinha ido.
                if r.is_err() && comandos.first().is_some_and(|c| c == "BEGIN") {
                    let _ = b.executar("ROLLBACK", 0);
                }
                (r, inicio.elapsed(), nova)
            },
            move |(r, t, nova)| Msg::Executado(rodada, r, t, nova),
        )
    }
}

impl Item {
    fn novo(id: u64, conexao: Conexao) -> Item {
        Item { id, conexao, senha: None, principal: Base::default(), bancos: None, bases: HashMap::new() }
    }

    /// Postgres ou MySQL sem banco escolhido: a árvore mostra os bancos do servidor.
    fn servidor(&self) -> bool {
        matches!(self.conexao.tipo, Tipo::Postgres | Tipo::MySql | Tipo::MongoDb) && self.conexao.banco.trim().is_empty()
    }
}

impl pelicano::App for App {
    type Msg = Msg;

    fn theme(&self) -> Theme {
        tema()
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Janela(cmd) => return Task::window(cmd),
            Msg::Puxar(d) => return Task::window(WindowCmd::DragResize(d)),
            Msg::Maximizada(m) => self.maximizada = m,
            Msg::Fechar => {
                self.gravar_rascunho();
                return Task::window(WindowCmd::Close);
            }
            Msg::Aux(a) => return self.update_aux(a),
            Msg::Nova => self.form = Some(Formulario::novo(None, Conexao::nova(Tipo::Postgres))),
            Msg::Editar(id) => {
                if let Some(i) = self.itens.iter().find(|i| i.id == id) {
                    // A senha já configurada vem preenchida: salvar a edição não obriga a digitar de novo.
                    let senha = i.senha.clone().or_else(|| i.conexao.lembrar_senha.then(|| featherdb_core::cofre::ler(&i.conexao)).flatten());
                    self.form = Some(Formulario::novo(Some(id), i.conexao.clone()).com_senha(senha.unwrap_or_default()));
                }
            }
            Msg::Excluir(id) => self.excluir = Some(id),
            Msg::ConfirmarExclusao(sim) => {
                if let (true, Some(id)) = (sim, self.excluir) {
                    if let Some(i) = self.itens.iter().find(|i| i.id == id) {
                        featherdb_core::cofre::apagar(&i.conexao);
                    }
                    self.itens.retain(|i| i.id != id);
                    if self.ativa.as_ref().is_some_and(|a| a.0 == id) {
                        self.ativa = self.itens.first().map(|i| (i.id, None));
                    }
                    self.gravar_conexoes();
                }
                self.excluir = None;
            }
            Msg::ConfirmarPerigo(sim) => {
                if let (true, Some((comandos, _))) = (sim, self.perigo.take()) {
                    return self.rodar_conferido(comandos);
                }
                self.perigo = None;
            }
            Msg::Filtro(f) => {
                self.filtro = f;
                // A busca também acha colunas: lê o dicionário dos bancos abertos na primeira vez.
                if self.filtro.trim().chars().count() >= 2 {
                    return self.carregar_dicionarios();
                }
            }
            Msg::Form(campo) => return self.update_form(campo),
            Msg::Senha(s) => {
                if let Some((_, senha)) = &mut self.pedir_senha {
                    *senha = s;
                }
            }
            Msg::LembrarSenha(v) => {
                let Some((id, _)) = self.pedir_senha else { return Task::none() };
                if let Some(i) = self.item(id) {
                    i.conexao.lembrar_senha = v;
                    if !v {
                        featherdb_core::cofre::apagar(&i.conexao);
                    }
                }
                self.gravar_conexoes();
            }
            Msg::ConfirmarSenha(sim) => {
                let Some((id, senha)) = self.pedir_senha.take() else { return Task::none() };
                if sim {
                    if let Some(i) = self.item(id) {
                        i.senha = Some(senha);
                    }
                    return self.conectar(id);
                }
            }
            Msg::Conectado(alvo, r) => {
                let Some(item) = self.item(alvo.0) else { return Task::none() };
                let principal = alvo.1.is_none();
                let servidor = item.servidor();
                let Some(base) = (match &alvo.1 {
                    None => Some(&mut item.principal),
                    Some(n) => item.bases.get_mut(n),
                }) else {
                    return Task::none();
                };
                // A conexão foi editada ou desconectada enquanto conectava: a resposta é velha.
                if !base.conectando {
                    return Task::none();
                }
                base.conectando = false;
                match r {
                    Ok(b) => {
                        base.banco = Some(b);
                        base.expandido = true;
                        self.erro = None;
                        // Só guarda a senha depois de ela funcionar.
                        if principal {
                            if let Some(item) = self.itens.iter().find(|i| i.id == alvo.0) {
                                if let (true, Some(senha)) = (item.conexao.lembrar_senha && App::precisa_senha(&item.conexao), &item.senha) {
                                    if let Err(e) = featherdb_core::cofre::gravar(&item.conexao, senha) {
                                        self.aviso = trf("A senha não foi guardada no cofre do sistema: {}", &[&e]);
                                    }
                                }
                            }
                            if servidor {
                                return self.carregar_bancos(alvo.0);
                            }
                        }
                        return self.carregar_tabelas(alvo);
                    }
                    Err(e) => {
                        let rotulo = item.conexao.rotulo();
                        let nome = match &alvo.1 {
                            Some(n) => format!("{rotulo} / {n}"),
                            None => rotulo,
                        };
                        // Senha errada: na próxima tentativa pede de novo.
                        if principal {
                            item.senha = None;
                        }
                        self.erro = Some(trf("Não conectou em {}: {}", &[&nome, &e]));
                    }
                }
            }
            Msg::Bancos(id, r) => match r {
                Ok(v) => {
                    if let Some(i) = self.item(id) {
                        i.bancos = Some(v);
                    }
                }
                Err(e) => self.erro = Some(trf("Não foi possível ler os bancos: {}", &[&e])),
            },
            Msg::Desconectar(id) => {
                if let Some(i) = self.item(id) {
                    i.principal = Base::default();
                    i.bases.clear();
                    i.bancos = None;
                }
            }
            Msg::Expandir(id) => {
                self.ativa = Some((id, None));
                let Some(i) = self.item(id) else { return Task::none() };
                if i.principal.banco.is_none() {
                    return self.conectar(id);
                }
                i.principal.expandido = !i.principal.expandido;
            }
            Msg::ExpandirBanco(id, nome) => {
                self.ativa = Some((id, Some(nome.clone())));
                match self.base_mut(&(id, Some(nome.clone()))) {
                    Some(b) if b.banco.is_some() => b.expandido = !b.expandido,
                    _ => return self.abrir_banco(id, nome),
                }
            }
            Msg::Atualizar(alvo) => {
                if alvo.1.is_none() && self.itens.iter().any(|i| i.id == alvo.0 && i.servidor()) {
                    return self.carregar_bancos(alvo.0);
                }
                if let Some(b) = self.base_mut(&alvo) {
                    b.colunas.clear();
                    b.tabelas_abertas.clear();
                    b.dicionario = None;
                }
                return self.carregar_tabelas(alvo);
            }
            Msg::Tabelas(alvo, r) => match r {
                Ok(t) => {
                    if let Some(b) = self.base_mut(&alvo) {
                        b.tabelas = t;
                    }
                }
                Err(e) => self.erro = Some(trf("Não foi possível ler as tabelas: {}", &[&e])),
            },
            Msg::ExpandirTabela(alvo, t) => {
                let tipo = self.tipo(alvo.0);
                let Some(b) = self.base_mut(&alvo) else { return Task::none() };
                let chave = t.qualificado(tipo);
                if !b.tabelas_abertas.remove(&chave) {
                    b.tabelas_abertas.insert(chave.clone());
                    if !b.colunas.contains_key(&chave) {
                        return self.carregar_colunas(alvo, t, false);
                    }
                }
            }
            Msg::Colunas(alvo, chave, r, exportar) => match r {
                Ok(c) => {
                    let texto = exportar.then(|| featherdb_core::estrutura::colunas_em_texto(&c, ','));
                    if let Some(b) = self.base_mut(&alvo) {
                        b.colunas.insert(chave.clone(), c);
                    }
                    if let Some(texto) = texto {
                        return App::salvar_texto(format!("{}-colunas.csv", chave.replace(['"', '`'], "")), texto);
                    }
                }
                Err(e) => self.erro = Some(e),
            },
            Msg::VerEstrutura(alvo, t) => {
                self.ativa = Some(alvo.clone());
                self.tabela_aberta = Some((alvo.clone(), t.clone()));
                self.aba = Aba::Estrutura;
                let tipo = self.tipo(alvo.0);
                if !self.base(&alvo).is_some_and(|b| b.colunas.contains_key(&t.qualificado(tipo))) {
                    return self.carregar_colunas(alvo, t, false);
                }
            }
            Msg::Aba(a) => {
                self.aba = a;
                if a == Aba::Sql {
                    return self.carregar_ddl();
                }
            }
            Msg::Ddl(chave, r) => {
                if let Some((c, v)) = &mut self.ddl {
                    if *c == chave {
                        *v = Some(r);
                    }
                }
            }
            Msg::CopiarDdl | Msg::DdlNoEditor => {
                let Some((_, Some(Ok(sql)))) = &self.ddl else { return Task::none() };
                if matches!(msg, Msg::DdlNoEditor) {
                    self.editor.insert(&format!("\n{sql}"));
                    self.gravar_rascunho();
                    self.aviso = tr("SQL da tabela posto no editor.").to_owned();
                } else {
                    self.aviso = match arboard::Clipboard::new().and_then(|mut c| c.set_text(sql.clone())) {
                        Ok(()) => tr("SQL copiado.").to_owned(),
                        Err(e) => trf("Não foi possível copiar: {}", &[&e]),
                    };
                }
            }
            Msg::Formatar => {
                if self.item_ativo().is_some_and(|i| i.conexao.tipo == Tipo::MongoDb) {
                    self.aviso = tr("Formatar é para SQL; comandos do MongoDB ficam como estão.").to_owned();
                    return Task::none();
                }
                // Com seleção, formata só ela; sem, o editor inteiro. Ctrl+Z desfaz.
                match self.editor.selection().filter(|s| !s.trim().is_empty()) {
                    Some(s) => self.editor.insert(featherdb_core::formatar::formatar(&s).trim_end()),
                    None => self.editor.set_text(&featherdb_core::formatar::formatar(&self.editor.text())),
                }
                self.gravar_rascunho();
            }
            Msg::Explicar => {
                let Some(tipo) = self.item_ativo().map(|i| i.conexao.tipo) else { return Task::none() };
                let prefixo = match tipo {
                    Tipo::Postgres | Tipo::MySql => "EXPLAIN ",
                    Tipo::Sqlite => "EXPLAIN QUERY PLAN ",
                    Tipo::Odbc | Tipo::MongoDb => {
                        self.aviso = tr("O plano da consulta é para PostgreSQL, MySQL e SQLite.").to_owned();
                        return Task::none();
                    }
                };
                let Some(c) = featherdb_core::dividir(&self.comando_atual()).into_iter().next() else { return Task::none() };
                self.tabela_aberta = None;
                self.aba = Aba::Dados;
                return self.rodar(vec![format!("{prefixo}{c}")]);
            }
            Msg::EditarColuna(c) => self.abrir_edicao(c),
            Msg::Edicao(c) => return self.update_edicao(c),
            Msg::CopiarColunas | Msg::ExportarColunas => {
                let Some((alvo, t)) = &self.tabela_aberta else { return Task::none() };
                let chave = t.qualificado(self.tipo(alvo.0));
                let Some(cols) = self.base(alvo).and_then(|b| b.colunas.get(&chave)) else { return Task::none() };
                if matches!(msg, Msg::ExportarColunas) {
                    let texto = featherdb_core::estrutura::colunas_em_texto(cols, ',');
                    return App::salvar_texto(format!("{}-colunas.csv", t.nome), texto);
                }
                let texto = featherdb_core::estrutura::colunas_em_texto(cols, '\t');
                self.aviso = match arboard::Clipboard::new().and_then(|mut c| c.set_text(texto)) {
                    Ok(()) => trf("{} colunas copiadas (cole na planilha).", &[&cols.len()]),
                    Err(e) => trf("Não foi possível copiar: {}", &[&e]),
                };
            }
            Msg::ExportarColunasDe(alvo, t) => {
                let chave = t.qualificado(self.tipo(alvo.0));
                match self.base(&alvo).and_then(|b| b.colunas.get(&chave)) {
                    Some(cols) => return App::salvar_texto(format!("{}-colunas.csv", t.nome), featherdb_core::estrutura::colunas_em_texto(cols, ',')),
                    None => return self.carregar_colunas(alvo, t, true),
                }
            }
            Msg::ExportarDicionario(alvo) => {
                let nome = alvo.1.clone().or_else(|| self.itens.iter().find(|i| i.id == alvo.0).map(|i| i.conexao.rotulo())).unwrap_or_default();
                if let Some(d) = self.base(&alvo).and_then(|b| b.dicionario.as_ref()) {
                    return App::salvar_texto(format!("{nome}-dicionario.csv"), featherdb_core::estrutura::dicionario_em_texto(d, ','));
                }
                if let Some(b) = self.base_mut(&alvo) {
                    b.lendo_dicionario = true;
                }
                return self.ler_dicionario(alvo, true);
            }
            Msg::Dicionario(alvo, r, exportar) => {
                let nome = alvo.1.clone().or_else(|| self.itens.iter().find(|i| i.id == alvo.0).map(|i| i.conexao.rotulo())).unwrap_or_default();
                let Some(b) = self.base_mut(&alvo) else { return Task::none() };
                b.lendo_dicionario = false;
                match r {
                    Ok(d) => {
                        let texto = exportar.then(|| featherdb_core::estrutura::dicionario_em_texto(&d, ','));
                        b.dicionario = Some(d);
                        if let Some(texto) = texto {
                            return App::salvar_texto(format!("{nome}-dicionario.csv"), texto);
                        }
                    }
                    Err(e) => {
                        b.dicionario = Some(Vec::new());
                        self.erro = Some(trf("Não foi possível ler as colunas do banco: {}", &[&e]));
                    }
                }
            }
            Msg::Parar => {
                if !self.executando {
                    return Task::none();
                }
                match self.cancelar.take() {
                    // O banco interrompe e a resposta chega como erro ("cancelado").
                    Some(c) => {
                        self.aviso = tr("Parando a consulta…").to_owned();
                        return em_segundo_plano(move || c(), |_| Msg::FecharMenu);
                    }
                    // Driver sem cancelamento (ODBC, MongoDB): larga a conexão ocupada e abre outra.
                    None => {
                        self.rodada += 1;
                        self.executando = false;
                        self.erro = Some(tr("Consulta parada. A conexão foi refeita.").to_owned());
                        let Some(alvo) = self.ativa.clone() else { return Task::none() };
                        if let Some(b) = self.base_mut(&alvo) {
                            b.banco = None;
                        }
                        return match alvo.1 {
                            None => self.conectar(alvo.0),
                            Some(n) => self.abrir_banco(alvo.0, n),
                        };
                    }
                }
            }
            Msg::Historico(h) => {
                self.historico = match (h, self.historico.take()) {
                    (None, _) => None,
                    (Some(b), Some((_, lista))) => Some((b, lista)),
                    (Some(b), None) => Some((b, historico::ler())),
                }
            }
            Msg::UsarHistorico(sql) => {
                self.historico = None;
                self.editor.insert(&format!("{sql}\n"));
                self.gravar_rascunho();
            }
            Msg::Ordenar(c) => {
                // Crescente, decrescente, e de volta à ordem do banco.
                self.ordenacao = match self.ordenacao {
                    Some((o, true)) if o == c => Some((c, false)),
                    Some((o, false)) if o == c => None,
                    _ => Some((c, true)),
                };
                self.reordenar();
            }
            Msg::FiltroGrade(f) => {
                self.filtro_grade = f;
                self.reordenar();
            }
            Msg::Rolou(x, y, w, h) => {
                self.rolagem = (x, y);
                self.vista = (w, h);
            }
            Msg::AbrirTabela(alvo, t) => {
                let tipo = self.tipo(alvo.0);
                self.ativa = Some(alvo.clone());
                self.tabela_aberta = Some((alvo.clone(), t.clone()));
                self.aba = Aba::Dados;
                // As colunas também, para a aba Estrutura já estar pronta.
                let colunas = if self.base(&alvo).is_some_and(|b| b.colunas.contains_key(&t.qualificado(tipo))) { Task::none() } else { self.carregar_colunas(alvo, t.clone(), false) };
                // Com LIMIT o banco não manda a tabela inteira (o ODBC fica sem: cada banco escreve de um jeito).
                let sql = match tipo {
                    Tipo::MongoDb => format!("db.getCollection({:?}).find({{}}).limit({LIMITE_LINHAS})", t.nome),
                    Tipo::Odbc => format!("SELECT * FROM {}", t.qualificado(tipo)),
                    _ => format!("SELECT * FROM {} LIMIT {LIMITE_LINHAS}", t.qualificado(tipo)),
                };
                let rodar = self.rodar(vec![sql]);
                return Task::batch([rodar, colunas]);
            }
            Msg::AbrirMenu(m, p) => self.menu = Some((m, p)),
            Msg::FecharMenu => self.menu = None,
            Msg::DoMenu(m) => {
                self.menu = None;
                return self.update(*m);
            }
            Msg::Conectar(id) => return self.conectar(id),
            Msg::EscreverSelect(alvo, t) => {
                let tipo = self.tipo(alvo.0);
                let Some(b) = self.base(&alvo) else { return Task::none() };
                let colunas = b.colunas.get(&t.qualificado(tipo)).filter(|c| !c.is_empty()).map_or("*".to_owned(), |c| {
                    c.iter().map(|c| featherdb_core::identificador(&c.nome, tipo)).collect::<Vec<_>>().join(", ")
                });
                self.ativa = Some(alvo);
                let texto = if tipo == Tipo::MongoDb {
                    format!("db.getCollection({:?}).find({{}})\n", t.nome)
                } else {
                    format!("SELECT {colunas}\nFROM {}\nWHERE ;\n", t.qualificado(tipo))
                };
                self.editor.insert(&texto);
                self.gravar_rascunho();
            }
            Msg::Contar(alvo, t) => {
                let tipo = self.tipo(alvo.0);
                self.ativa = Some(alvo);
                return self.rodar(vec![if tipo == Tipo::MongoDb {
                    format!("db.getCollection({:?}).countDocuments({{}})", t.nome)
                } else {
                    format!("SELECT COUNT(*) AS total FROM {}", t.qualificado(tipo))
                }]);
            }
            Msg::CopiarTexto(texto) => {
                self.aviso = match arboard::Clipboard::new().and_then(|mut c| c.set_text(texto.clone())) {
                    Ok(()) => trf("Copiado: {}", &[&texto]),
                    Err(e) => trf("Não foi possível copiar: {}", &[&e]),
                };
            }
            Msg::TamanhoJanela(t) => self.tamanho = t,
            Msg::Executar => {
                self.tabela_aberta = None;
                self.aba = Aba::Dados;
                // Cada comando vai sozinho: nem todo driver ODBC aceita vários de uma vez.
                return self.rodar(featherdb_core::dividir(&self.comando_atual()));
            }
            Msg::EditorMudou => {
                self.gravar_rascunho();
                return self.sugerir();
            }
            Msg::Sugestao(t) => self.tecla_sugestao(t),
            Msg::Celula(a) => return self.update_celula(a),
            Msg::ExecutarTudo => {
                self.tabela_aberta = None;
                self.aba = Aba::Dados;
                return self.rodar(featherdb_core::dividir(&self.editor.text()));
            }
            Msg::Executado(rodada, r, t, nova) => {
                if rodada != self.rodada {
                    return Task::none();
                }
                if nova {
                    self.aviso = tr("A conexão tinha caído e foi refeita.").to_owned();
                }
                self.cancelar = None;
                self.executando = false;
                self.tempo = Some(t);
                self.versao_resultado += 1;
                self.rolagem = (0.0, 0.0);
                match r {
                    Ok(r) => {
                        self.erro = None;
                        // Foi um ALTER TABLE da aba Estrutura: lê a tabela de novo e volta para a estrutura.
                        if let Some((alvo, t)) = self.recarregar.take() {
                            let chave = t.qualificado(self.tipo(alvo.0));
                            if let Some(b) = self.base_mut(&alvo) {
                                b.colunas.remove(&chave);
                                b.dicionario = None;
                            }
                            self.tabela_aberta = Some((alvo.clone(), t.clone()));
                            self.aba = Aba::Estrutura;
                            self.ddl = None;
                            self.aviso = tr("Coluna alterada.").to_owned();
                            return self.carregar_colunas(alvo, t, false);
                        }
                        // Gravou as células editadas: lê a tabela de novo para mostrar como ficou no banco.
                        if std::mem::take(&mut self.celulas.recarregar) {
                            self.celulas = Default::default();
                            self.aviso = tr("Mudanças salvas no banco.").to_owned();
                            if let Some((alvo, t)) = self.tabela_aberta.clone() {
                                return self.update(Msg::AbrirTabela(alvo, t));
                            }
                        }
                        self.celulas = Default::default();
                        self.resultado = Some(r);
                        self.ordenacao = None;
                        self.filtro_grade.clear();
                        self.reordenar();
                    }
                    Err(e) => {
                        // Falhou ao gravar: a grade fica e as mudanças continuam marcadas para corrigir e tentar de novo.
                        if std::mem::take(&mut self.celulas.recarregar) {
                            self.aviso = trf("Não salvou (nada mudou no banco): {}", &[&e]);
                            return Task::none();
                        }
                        self.recarregar = None;
                        self.aba = Aba::Dados;
                        self.erro = Some(e);
                    }
                }
            }
            Msg::Copiar => {
                if let Some(r) = &self.resultado {
                    let texto = grade::separado(r, &self.visiveis, '\t');
                    self.aviso = match arboard::Clipboard::new().and_then(|mut c| c.set_text(texto)) {
                        Ok(()) => trf("{} linhas copiadas (cole na planilha).", &[&self.visiveis.len()]),
                        Err(e) => trf("Não foi possível copiar: {}", &[&e]),
                    };
                }
            }
            Msg::ExportarCsv => {
                let Some(r) = &self.resultado else { return Task::none() };
                let csv = grade::separado(r, &self.visiveis, ',');
                return Task::perform(
                    async move {
                        let arq = rfd::AsyncFileDialog::new().add_filter("CSV", &["csv"]).set_file_name("resultado.csv").save_file().await?;
                        let caminho = arq.path().to_path_buf();
                        Some(std::fs::write(&caminho, csv).map(|_| caminho).map_err(|e| e.to_string()))
                    },
                    Msg::Exportado,
                );
            }
            Msg::Exportado(r) => {
                self.aviso = match r {
                    None => String::new(),
                    Some(Ok(c)) => trf("Salvo em {}", &[&c.display()]),
                    Some(Err(e)) => trf("Não foi possível salvar: {}", &[&e]),
                }
            }
        }
        Task::none()
    }

    fn on_event(&self, ev: &Event) -> Option<Msg> {
        match ev {
            Event::CloseRequested => Some(Msg::Fechar),
            Event::Maximized(m) => Some(Msg::Maximizada(*m)),
            Event::Resized(t) => Some(Msg::TamanhoJanela(*t)),
            _ => None,
        }
    }

    fn on_unhandled_event(&self, id: pelicano::WindowId, ev: &Event) -> Option<Msg> {
        let Event::Key { key, modifiers, pressed: true, .. } = ev else { return None };
        if id != pelicano::WindowId::MAIN {
            return Some(Msg::Aux(Aux::Teclado(id, key.clone(), *modifiers)));
        }
        if let Some(feather_ui::atalhos::AcaoGlobal::Ajuda) = feather_ui::atalhos::acao_global(key, *modifiers) {
            return Some(Msg::Aux(Aux::Abrir(Janela::Ajuda)));
        }
        if self.menu.is_some() {
            return matches!(key, Key::Named(NamedKey::Escape)).then_some(Msg::FecharMenu);
        }
        if self.edicao.is_some() {
            return matches!(key, Key::Named(NamedKey::Escape)).then_some(Msg::Edicao(CampoColuna::Cancelar));
        }
        if self.historico.is_some() {
            return matches!(key, Key::Named(NamedKey::Escape)).then_some(Msg::Historico(None));
        }
        if self.celulas.confirmar.is_some() {
            return matches!(key, Key::Named(NamedKey::Escape)).then_some(Msg::Celula(celulas::AcaoCelula::Confirmar(false)));
        }
        if self.celulas.editando.is_some() && matches!(key, Key::Named(NamedKey::Escape)) {
            return Some(Msg::Celula(celulas::AcaoCelula::Cancelar));
        }
        if self.pedir_senha.is_some() || self.form.is_some() || self.excluir.is_some() || self.perigo.is_some() {
            return matches!(key, Key::Named(NamedKey::Escape)).then(|| match () {
                _ if self.perigo.is_some() => Msg::ConfirmarPerigo(false),
                _ if self.pedir_senha.is_some() => Msg::ConfirmarSenha(false),
                _ if self.excluir.is_some() => Msg::ConfirmarExclusao(false),
                _ => Msg::Form(Campo::Cancelar),
            });
        }
        atalho(key, *modifiers)
    }

    fn view_window(&self, id: pelicano::WindowId) -> Element<'_, Msg> {
        self.view_aux(id).unwrap_or_else(|| self.view())
    }

    fn on_window_event(&self, id: pelicano::WindowId, ev: &Event) -> Option<Msg> {
        if id == pelicano::WindowId::MAIN {
            return self.on_event(ev);
        }
        matches!(ev, Event::CloseRequested).then(|| Msg::Aux(Aux::Fechar(id)))
    }

    fn view(&self) -> Element<'_, Msg> {
        let corpo = row![self.lateral(), divisoria_vertical(), self.area_consulta()].height(Length::FILL);
        let tudo = column![self.barra(), corpo, self.rodape()];
        let janela = cromo::moldura_redimensionavel_com(tudo.into(), self.maximizada, Msg::Puxar);
        if self.menu.is_some() {
            return self.view_menu(janela);
        }
        if self.edicao.is_some() {
            return self.view_edicao(janela);
        }
        if self.historico.is_some() {
            return self.view_historico(janela);
        }
        if self.celulas.confirmar.is_some() {
            return self.view_salvar_celulas(janela);
        }
        if let Some(f) = &self.form {
            return f.view().sobre(janela);
        }
        if let Some((id, senha)) = &self.pedir_senha {
            let rotulo = self.itens.iter().find(|i| i.id == *id).map(|i| i.conexao.rotulo()).unwrap_or_default();
            let campo = pelicano::text_input(tr("Senha"), senha.as_str()).secure(true).on_input(Msg::Senha).on_submit(Msg::ConfirmarSenha(true)).autofocus(1).padding([7, 10]);
            let lembrar = self.itens.iter().find(|i| i.id == *id).is_some_and(|i| i.conexao.lembrar_senha);
            let caixa = pelicano::checkbox(tr("Lembrar a senha neste computador (no cofre de senhas do sistema)"), lembrar).on_toggle(Msg::LembrarSenha).size(13);
            let corpo = column![campo, caixa].spacing(12);
            return feather_ui::dialogo::Dialogo::new(trf("Conectar em {}", &[&rotulo]), corpo)
                .icone("lock")
                .largura(420.0)
                .ao_fechar(Msg::ConfirmarSenha(false))
                .secundario(tr("Cancelar"), Some(Msg::ConfirmarSenha(false)))
                .primario(tr("Conectar"), Some(Msg::ConfirmarSenha(true)))
                .sobre(janela);
        }
        if let Some((comandos, palavra)) = &self.perigo {
            let sql = comandos.iter().find(|c| featherdb_core::sem_where(c).is_some()).map_or("", String::as_str);
            let previa: String = sql.chars().take(300).collect();
            let corpo = column![
                text(trf("Este {} não tem WHERE: vale para todas as linhas da tabela.", &[palavra])).size(13),
                container(text(previa).size(12).font(Font::MONO)).padding(10).width(Length::FILL).style(|_| Style::default().background(COR_PAINEL_FAIXA()).border(Border::rounded(6.0))),
            ]
            .spacing(10);
            return feather_ui::dialogo::Dialogo::new(tr("Mexer na tabela inteira?"), corpo)
                .icone("alert-triangle")
                .largura(480.0)
                .ao_fechar(Msg::ConfirmarPerigo(false))
                .secundario(tr("Cancelar"), Some(Msg::ConfirmarPerigo(false)))
                .primario(tr("Rodar mesmo assim"), Some(Msg::ConfirmarPerigo(true)))
                .sobre(janela);
        }
        if let Some(id) = self.excluir {
            let rotulo = self.itens.iter().find(|i| i.id == id).map(|i| i.conexao.rotulo()).unwrap_or_default();
            let corpo = text(trf("A conexão “{}” sai da lista. O banco de dados não é mexido.", &[&rotulo])).size(13);
            return feather_ui::dialogo::Dialogo::new(tr("Excluir conexão?"), corpo)
                .icone("trash")
                .largura(420.0)
                .ao_fechar(Msg::ConfirmarExclusao(false))
                .secundario(tr("Cancelar"), Some(Msg::ConfirmarExclusao(false)))
                .primario(tr("Excluir"), Some(Msg::ConfirmarExclusao(true)))
                .sobre(janela);
        }
        janela
    }
}

/// Os atalhos da janela principal (os mesmos do DBeaver, para quem vem de lá).
pub(crate) fn atalho(key: &Key, m: Modifiers) -> Option<Msg> {
    match key {
        Key::Named(NamedKey::Enter) if m.control_key() && m.shift_key() => Some(Msg::ExecutarTudo),
        Key::Named(NamedKey::Enter) if m.control_key() => Some(Msg::Executar),
        Key::Named(NamedKey::F5) => Some(Msg::ExecutarTudo),
        Key::Character(c) if m.alt_key() && c.eq_ignore_ascii_case("x") => Some(Msg::ExecutarTudo),
        Key::Character(c) if m.control_key() && m.shift_key() && c.eq_ignore_ascii_case("f") => Some(Msg::Formatar),
        Key::Character(c) if m.control_key() && c.eq_ignore_ascii_case("h") => Some(Msg::Historico(Some(String::new()))),
        _ => None,
    }
}

impl App {
    fn barra(&self) -> Element<'_, Msg> {
        let barra = row![
            marca(18.0),
            text("FeatherDB").size(14).font(Font::SEMIBOLD),
            horizontal_space(),
            acao("info-circle", tr("Ajuda (F1)"), Some(Msg::Aux(Aux::Abrir(Janela::Ajuda)))),
            acao("settings", tr("Configurações"), Some(Msg::Aux(Aux::Abrir(Janela::Configuracoes)))),
            Space::new(8, 0),
        ]
        .spacing(8)
        .align(Align::Center);
        let barra = container(barra).padding([0, 14]).align_y(Align::Center).width(Length::FILL).height(cromo::BARRA_ALTURA).style(estilo_container_topo);
        cromo::barra_integrada(
            barra.into(),
            AcoesJanela { arrastar: Msg::Janela(WindowCmd::Drag), minimizar: Msg::Janela(WindowCmd::Minimize), maximizar: Msg::Janela(WindowCmd::ToggleMaximize), fechar: Msg::Fechar },
        )
    }

    fn item_ativo(&self) -> Option<&Item> {
        self.ativa.as_ref().and_then(|(id, _)| self.itens.iter().find(|i| i.id == *id))
    }

    /// As tabelas (e as colunas das abertas) de um banco, com o recuo do nível na árvore.
    fn arvore_tabelas<'a>(&'a self, alvo: Alvo, base: &'a Base, tipo: Tipo, recuo: f32) -> Vec<Element<'a, Msg>> {
        let mut saida = Vec::new();
        // Bancos como o Datasphere têm milhares de visões: o filtro acha, e a árvore mostra no máximo
        // MAX_ARVORE por banco para a tela continuar leve.
        let filtro = self.filtro.trim().to_lowercase();
        let achadas: Vec<&Tabela> = base.tabelas.iter().filter(|t| filtro.is_empty() || t.nome.to_lowercase().contains(&filtro) || t.esquema.to_lowercase().contains(&filtro)).collect();
        if base.tabelas.is_empty() {
            saida.push(container(text(tr("nenhuma tabela")).size(12).color(COR_TEXTO_MUTED())).padding(pelicano::Padding { top: 3.0, right: 0.0, bottom: 3.0, left: recuo + 34.0 }).into());
        }
        // Um esquema só (o `public` do Postgres, quase sempre): só o nome da tabela. Mais de um: um título
        // por esquema, com as tabelas embaixo (o catálogo já vem ordenado por esquema).
        let varios = base.tabelas.iter().any(|t| t.esquema != base.tabelas[0].esquema);
        let mut esquema_atual: Option<&str> = None;
        for t in achadas.iter().take(MAX_ARVORE).copied() {
            if varios && esquema_atual != Some(t.esquema.as_str()) {
                esquema_atual = Some(&t.esquema);
                saida.push(
                    container(text(t.esquema.clone()).size(11.5).font(Font::SEMIBOLD).color(COR_TEXTO_MUTED()))
                        .padding(pelicano::Padding { top: 6.0, right: 0.0, bottom: 2.0, left: recuo + 20.0 })
                        .into(),
                );
            }
            let chave = t.qualificado(tipo);
            let aberta = base.tabelas_abertas.contains(&chave);
            let nome = t.nome.clone();
            let linha = row![
                Space::new(recuo + if varios { 22.0 } else { 14.0 }, 0),
                icone_tabler(if aberta { SETA_BAIXO } else { SETA_DIREITA }, 12.0).color(COR_TEXTO_MUTED()),
                if t.visao { icone_tabler(VISAO, 15.0).color(COR_TEXTO_MUTED()) } else { feather_ui::icones::icone_traco("table", 15.0).color(COR_TEXTO_MUTED()) },
                text(nome).size(12.5).no_wrap().ellipsis(),
            ]
            .width(Length::FILL)
            .spacing(6)
            .align(Align::Center);
            let botao = button(linha).padding([4, 6]).width(Length::FILL).style(estilo_botao_pequeno(false)).on_press(Msg::ExpandirTabela(alvo.clone(), t.clone()));
            let (a2, tab) = (alvo.clone(), t.clone());
            let botao = mouse_area(botao).on_double_click(Msg::AbrirTabela(alvo.clone(), t.clone())).on_right_press_at(move |p| Msg::AbrirMenu(Menu::Tabela(a2.clone(), tab.clone()), p));
            saida.push(tooltip(botao, tr("Clique duplo mostra os dados"), TooltipPosition::Right).padding([4, 8]).style(estilo_dica).into());
            if aberta {
                for c in base.colunas.get(&chave).map(Vec::as_slice).unwrap_or_default() {
                    saida.push(
                        row![
                            Space::new(recuo + 40.0, 0),
                            grade::icone_tipo(&c.tipo, c.chave, 13.0),
                            text(c.nome.as_str()).size(12).no_wrap().ellipsis(),
                            horizontal_space(),
                            text(c.tipo.to_lowercase()).size(11).font(Font::MONO).color(COR_TEXTO_MUTED()).no_wrap(),
                        ]
                        .spacing(6)
                        .align(Align::Center)
                        .padding([3, 10])
                        .into(),
                    );
                }
            }
        }
        if achadas.len() > MAX_ARVORE {
            let resto = achadas.len() - MAX_ARVORE;
            saida.push(container(text(trf("mais {}; use o filtro para achar", &[&resto])).size(12).color(COR_TEXTO_MUTED())).padding([4, 24]).into());
        }
        saida
    }

    /// Um banco do servidor na árvore (conexão sem banco escolhido).
    fn linha_banco<'a>(&'a self, item: &'a Item, nome: &'a str) -> Element<'a, Msg> {
        let base = item.bases.get(nome);
        let aberto = base.is_some_and(|b| b.banco.is_some() && b.expandido);
        let ativo = self.ativa.as_ref().is_some_and(|(id, b)| *id == item.id && b.as_deref() == Some(nome));
        let mut linha = row![
            Space::new(6, 0),
            icone_tabler(if aberto { SETA_BAIXO } else { SETA_DIREITA }, 12.0).color(COR_TEXTO_MUTED()),
            icone_tabler(BANCO, 15.0).color(if base.is_some_and(|b| b.banco.is_some()) { COR_AZUL_ACENTO() } else { COR_TEXTO_MUTED() }),
            text(nome.to_owned()).size(12.5).font(if ativo { Font::SEMIBOLD } else { Font::DEFAULT }).no_wrap().ellipsis().width(Length::FILL),
        ]
        .width(Length::FILL)
        .spacing(6)
        .align(Align::Center);
        if base.is_some_and(|b| b.conectando) {
            linha = linha.push(text(tr("conectando…")).size(11).color(COR_TEXTO_MUTED()));
        }
        let (id, n) = (item.id, nome.to_owned());
        let botao = button(linha).padding([4, 6]).width(Length::FILL).style(estilo_botao_pequeno(ativo)).on_press(Msg::ExpandirBanco(item.id, nome.to_owned()));
        mouse_area(botao).on_right_press_at(move |p| Msg::AbrirMenu(Menu::Banco(id, n.clone()), p)).into()
    }

    /// A árvore: conexões › tabelas › colunas.
    fn lateral(&self) -> Element<'_, Msg> {
        let mut lista = column![].spacing(2);
        if self.itens.is_empty() {
            lista = lista.push(
                column![
                    text(tr("Nenhuma conexão ainda.")).size(13).font(Font::SEMIBOLD),
                    text(tr("Crie uma para PostgreSQL, MySQL, SQLite, MongoDB ou qualquer banco com driver ODBC (Datasphere, TDV, SQL Server…).")).size(12).color(COR_TEXTO_MUTED()).line_height(1.45),
                    feather_ui::dialogo::botao_primario(tr("Nova conexão"), Some(Msg::Nova)),
                ]
                .spacing(10)
                .padding([8, 10]),
            );
        }
        if let Some(achados) = self.resultados_busca() {
            lista = lista.push(achados);
        }
        for item in &self.itens {
            lista = lista.push(self.linha_conexao(item));
            if !item.principal.expandido || item.principal.banco.is_none() {
                continue;
            }
            let tipo = item.conexao.tipo;
            match &item.bancos {
                Some(bancos) => {
                    for nome in bancos {
                        lista = lista.push(self.linha_banco(item, nome));
                        if let Some(b) = item.bases.get(nome).filter(|b| b.expandido && b.banco.is_some()) {
                            for e in self.arvore_tabelas((item.id, Some(nome.clone())), b, tipo, 14.0) {
                                lista = lista.push(e);
                            }
                        }
                    }
                }
                None if item.servidor() => lista = lista.push(container(text(tr("lendo os bancos…")).size(12).color(COR_TEXTO_MUTED())).padding([3, 40])),
                None => {
                    for e in self.arvore_tabelas((item.id, None), &item.principal, tipo, 0.0) {
                        lista = lista.push(e);
                    }
                }
            }
        }
        // As ações valem para a conexão ativa (a destacada na lista).
        let mut topo = row![text(tr("Conexões")).size(12).font(Font::SEMIBOLD).color(COR_TEXTO_MUTED()), horizontal_space()].spacing(0).align(Align::Center);
        if let Some(item) = self.item_ativo() {
            if item.principal.banco.is_some() {
                let alvo = self.ativa.clone().unwrap_or((item.id, None));
                topo = topo.push(acao_pequena(feather_ui::icones::icone_traco("refresh", 15.0), tr("Atualizar tabelas"), Some(Msg::Atualizar(alvo))));
                topo = topo.push(acao_pequena(icone_tabler(DESCONECTAR, 15.0), tr("Desconectar"), Some(Msg::Desconectar(item.id))));
            }
            topo = topo.push(acao_pequena(feather_ui::icones::icone_traco("pencil", 15.0), tr("Editar conexão"), Some(Msg::Editar(item.id))));
            topo = topo.push(acao_pequena(feather_ui::icones::icone_traco("trash", 15.0), tr("Excluir conexão"), Some(Msg::Excluir(item.id))));
        }
        let topo = topo
            .push(acao_pequena(feather_ui::icones::icone_traco("plus", 16.0), tr("Nova conexão"), Some(Msg::Nova)))
            .padding(pelicano::Padding { top: 10.0, right: 8.0, bottom: 6.0, left: 14.0 });
        // O filtro só aparece quando há tabelas para filtrar.
        let filtro: Element<'_, Msg> = if self.itens.iter().any(|i| !i.principal.tabelas.is_empty() || i.bases.values().any(|b| !b.tabelas.is_empty())) {
            container(pelicano::text_input(tr("Buscar tabela ou coluna"), self.filtro.as_str()).on_input(Msg::Filtro).padding([6, 10]).size(12.5)).padding(pelicano::Padding { top: 0.0, right: 10.0, bottom: 8.0, left: 10.0 }).into()
        } else {
            Space::new(0, 0).into()
        };
        container(column![topo, filtro, rolagem(container(lista).padding(pelicano::Padding { top: 0.0, right: 8.0, bottom: 8.0, left: 8.0 }))])
            .width(LARGURA_LATERAL)
            .height(Length::FILL)
            .style(|_| Style::default().background(COR_PAINEL_FAIXA()))
            .into()
    }

    fn linha_conexao<'a>(&'a self, item: &'a Item) -> Element<'a, Msg> {
        let ativa = self.ativa == Some((item.id, None));
        let (estado, ligada) = if item.principal.conectando {
            (tr("conectando…"), false)
        } else if item.principal.banco.is_some() {
            (tr("conectada"), true)
        } else {
            (tr("desconectada"), false)
        };
        let ponto_cor = if ligada { COR_AZUL_ACENTO() } else { COR_BORDA() };
        let ponto = container(Space::new(7, 7)).style(move |_| Style::default().background(ponto_cor).border(Border::rounded(3.5)));
        let linha = row![
            selo(item.conexao.tipo),
            column![
                text(item.conexao.rotulo()).size(13).font(if ativa { Font::SEMIBOLD } else { Font::DEFAULT }).no_wrap().ellipsis(),
                row![ponto, text(estado).size(11).color(COR_TEXTO_MUTED())].spacing(5).align(Align::Center),
            ]
            .spacing(2)
            .width(Length::FILL),
        ]
        .width(Length::FILL)
        .spacing(9)
        .align(Align::Center);
        let id = item.id;
        let botao = button(linha).padding([6, 8]).width(Length::FILL).style(estilo_botao_pequeno(ativa)).on_press(Msg::Expandir(id));
        mouse_area(botao).on_right_press_at(move |p| Msg::AbrirMenu(Menu::Conexao(id), p)).into()
    }

    fn area_consulta(&self) -> Element<'_, Msg> {
        let cor_chave = COR_AZUL_ACENTO();
        let cor_comentario = COR_TEXTO_MUTED();
        let editor = text_editor(&self.editor)
            .font(Font::MONO)
            .size(13.5)
            .wrap(false)
            .line_numbers(true)
            .placeholder(tr("Escreva o SQL aqui. Ctrl+Enter roda o comando onde está o cursor."))
            .on_key(self.teclas_editor())
            .on_change(Msg::EditorMudou)
            .colors(move |linha| grade::cores_sql(linha, cor_chave, cor_comentario));
        let mut texto = pelicano::stack![container(editor).height(Length::FILL).width(Length::FILL)];
        if let Some(s) = self.view_sugestao() {
            texto = texto.push(s);
        }
        let editor = painel(column![self.cabecalho_editor(), divisoria(), texto]).height(Length::Fill(2));
        let resultado = painel(self.view_resultado()).height(Length::Fill(3));
        container(column![editor, resultado].spacing(8).width(Length::FILL))
            .padding(8)
            .width(Length::FILL)
            .height(Length::FILL)
            .style(|_| Style::default().background(COR_FUNDO_JANELA()))
            .into()
    }

    /// A faixa do editor: em que banco o SQL vai rodar e os botões de executar.
    fn cabecalho_editor(&self) -> Element<'_, Msg> {
        let pode = !self.executando;
        let onde: Element<'_, Msg> = match self.item_ativo() {
            Some(i) => {
                let banco = self.ativa.as_ref().and_then(|a| a.1.clone()).unwrap_or_default();
                row![selo(i.conexao.tipo), text(i.conexao.rotulo()).size(13).font(Font::SEMIBOLD), text(if banco.is_empty() { String::new() } else { format!("/ {banco}") }).size(13).color(COR_TEXTO_MUTED())]
                    .spacing(8)
                    .align(Align::Center)
                    .into()
            }
            None => text(tr("Nenhuma conexão escolhida")).size(13).color(COR_TEXTO_MUTED()).into(),
        };
        // Rodando, o Executar vira Parar.
        let (icone, rotulo, msg, dica) = if self.executando {
            ("player-stop", tr("Parar"), Msg::Parar, tr("Parar a consulta"))
        } else {
            ("player-play", tr("Executar"), Msg::Executar, tr("Executar o comando do cursor (Ctrl+Enter)"))
        };
        let executar = button(
            row![feather_ui::icones::icone_traco(icone, 15.0).color(COR_ACENTO_TEXTO(COR_AZUL_ACENTO())), text(rotulo).size(13).font(Font::SEMIBOLD)]
                .spacing(6)
                .align(Align::Center),
        )
        .padding([6, 14])
        .style(estilo_botao_primario)
        .on_press(msg);
        let formatar = acao_pequena(feather_ui::icones::icone_traco("wand", 15.0), tr("Formatar o SQL (Ctrl+Shift+F)"), Some(Msg::Formatar));
        let explicar = acao_pequena(feather_ui::icones::icone_traco("sitemap", 15.0), tr("Plano da consulta (EXPLAIN) do comando do cursor"), pode.then_some(Msg::Explicar));
        let historico = acao_pequena(feather_ui::icones::icone_traco("history", 15.0), tr("Histórico de consultas (Ctrl+H)"), Some(Msg::Historico(Some(String::new()))));
        let script = button(row![icone_tabler(EXECUTAR_TUDO, 15.0), text(tr("Script")).size(13)].spacing(6).align(Align::Center))
            .padding([6, 12])
            .style(estilo_botao_secundario)
            .on_press_maybe(pode.then_some(Msg::ExecutarTudo));
        row![
            onde,
            horizontal_space(),
            formatar,
            explicar,
            historico,
            tooltip(script, tr("Executar o script inteiro (Alt+X)"), TooltipPosition::Bottom).padding([4, 8]).style(estilo_dica),
            tooltip(executar, dica, TooltipPosition::Bottom).padding([4, 8]).style(estilo_dica),
        ]
        .spacing(8)
        .align(Align::Center)
        .padding([8, 12])
        .into()
    }

    /// O painel de baixo: o resultado da consulta, ou a tabela aberta com as abas Dados e Estrutura.
    fn view_resultado(&self) -> Element<'_, Msg> {
        let Some((alvo, t)) = &self.tabela_aberta else {
            let mut barra = row![text(tr("Resultado")).size(13).font(Font::SEMIBOLD)].spacing(8).align(Align::Center);
            barra = self.chips_e_acoes(barra);
            return column![barra.padding([6, 12]), divisoria(), self.corpo_dados()].into();
        };
        let tipo = self.tipo(alvo.0);
        let mongo = tipo == Tipo::MongoDb;
        let ncol = self.base(alvo).and_then(|b| b.colunas.get(&t.qualificado(tipo))).map(Vec::len);
        let aba = |rotulo: &str, a: Aba| {
            let ativa = self.aba == a;
            button(text(rotulo.to_owned()).size(13).font(if ativa { Font::SEMIBOLD } else { Font::DEFAULT })).padding([5, 12]).style(estilo_botao_pequeno(ativa)).on_press(Msg::Aba(a))
        };
        let nome = if t.esquema.is_empty() { t.nome.clone() } else { format!("{}.{}", t.esquema, t.nome) };
        let mut barra = row![
            if t.visao { icone_tabler(VISAO, 16.0) } else { feather_ui::icones::icone_traco("table", 16.0) },
            text(nome).size(13).font(Font::SEMIBOLD),
            Space::new(8, 0),
            aba(tr("Dados"), Aba::Dados),
            aba(if mongo { tr("Campos") } else { tr("Estrutura") }, Aba::Estrutura),
        ]
        .spacing(4)
        .align(Align::Center);
        if !mongo {
            barra = barra.push(aba("SQL", Aba::Sql));
        }
        barra = barra.push(Space::new(4, 0));
        let corpo = match self.aba {
            Aba::Dados => {
                barra = self.chips_e_acoes(barra);
                self.corpo_dados()
            }
            Aba::Sql => {
                barra = barra.push(horizontal_space()).push(self.acoes_ddl());
                self.view_ddl()
            }
            Aba::Estrutura => {
                let n = ncol.map(|n| if mongo { trf("{} campos", &[&n]) } else { trf("{} colunas", &[&n]) }).unwrap_or_default();
                barra = barra.push(chip(n)).push(horizontal_space()).push(self.acoes_estrutura());
                self.view_estrutura()
            }
        };
        column![barra.padding([6, 12]), divisoria(), corpo].into()
    }

    /// Contagem, tempo, copiar e exportar do resultado (quando há linhas).
    fn chips_e_acoes<'a>(&'a self, mut barra: pelicano::Row<'a, Msg>) -> pelicano::Row<'a, Msg> {
        let Some(r) = self.resultado.as_ref().filter(|r| !r.colunas.is_empty() && !self.executando && self.erro.is_none()) else {
            return barra.push(horizontal_space());
        };
        let contagem = match () {
            _ if self.visiveis.len() < r.linhas.len() => trf("{} de {} linhas", &[&self.visiveis.len(), &r.linhas.len()]),
            _ if r.cortado => trf("Primeiras {} linhas", &[&r.linhas.len()]),
            _ => trf("{} linhas", &[&r.linhas.len()]),
        };
        let tempo = self.tempo.map_or(String::new(), |t| trf("{} ms", &[&t.as_millis()]));
        let filtro = pelicano::text_input(tr("Filtrar linhas"), &self.filtro_grade).on_input(Msg::FiltroGrade).size(12.5).padding([4, 8]).width(180);
        barra = barra.push(chip(contagem)).push(chip(tempo)).push(horizontal_space());
        if self.chaves_editaveis().is_some() {
            use celulas::AcaoCelula as A;
            if self.celulas.editando.is_some() {
                barra = barra.push(button(text("NULL").size(12).font(Font::MONO)).padding([4, 10]).style(estilo_botao_secundario).on_press(Msg::Celula(A::Nulo)));
            }
            let n = self.celulas.pendentes.len();
            if n > 0 {
                barra = barra
                    .push(chip(trf("{} célula(s) mudada(s)", &[&n])))
                    .push(button(text(tr("Descartar")).size(12.5)).padding([4, 10]).style(estilo_botao_secundario).on_press(Msg::Celula(A::Descartar)))
                    .push(button(text(tr("Salvar")).size(12.5).font(Font::SEMIBOLD)).padding([4, 12]).style(estilo_botao_primario).on_press(Msg::Celula(A::Salvar)));
            } else if self.celulas.editando.is_none() {
                barra = barra.push(text(tr("clique duplo edita")).size(11.5).color(COR_TEXTO_MUTED()));
            }
        }
        barra = barra.push(filtro);
        barra
            .push(acao_pequena(feather_ui::icones::icone_traco("copy", 15.0), tr("Copiar tudo (cola na planilha)"), Some(Msg::Copiar)))
            .push(acao_pequena(feather_ui::icones::icone_traco("file-export", 15.0), tr("Exportar CSV"), Some(Msg::ExportarCsv)))
    }

    /// O resultado: rodando, erro, vazio, contagem de linhas afetadas ou a grade.
    fn corpo_dados(&self) -> Element<'_, Msg> {
        if self.executando {
            return vazio("clock", tr("Rodando…"), "");
        }
        if let Some(e) = &self.erro {
            let barra = container(Space::new(3, Length::FILL)).style(|_| Style::default().background(COR_ERRO()));
            let texto = column![text(tr("Erro")).size(13).font(Font::SEMIBOLD).color(COR_ERRO()), text(e.as_str()).size(12.5).font(Font::MONO)].spacing(6);
            return rolagem(row![barra, texto].spacing(12).padding(16)).into();
        }
        let Some(r) = &self.resultado else {
            return vazio("table", tr("Nenhum resultado ainda"), tr("Escreva uma consulta e aperte Ctrl+Enter."));
        };
        if r.colunas.is_empty() {
            let n = r.afetadas.unwrap_or(0);
            return vazio("check", tr("Pronto"), &trf("{} linha(s) afetada(s).", &[&n]));
        }
        let celulas = self.chaves_editaveis().map(|_| &self.celulas);
        grade::view(r, &self.visiveis, self.ordenacao, celulas, self.rolagem, self.vista, self.versao_resultado)
    }

    fn rodape(&self) -> Element<'_, Msg> {
        let estado = match self.item_ativo() {
            Some(i) if i.principal.banco.is_some() => trf("Conectado a {} · {}", &[&i.conexao.rotulo(), &i.conexao.tipo.nome()]),
            Some(i) if i.principal.conectando => trf("Conectando a {}…", &[&i.conexao.rotulo()]),
            _ => tr("Sem conexão").to_owned(),
        };
        let linha = row![text(estado).size(12).color(COR_TEXTO_MUTED()), Space::new(16, 0), text(self.aviso.as_str()).size(12)].align(Align::Center).padding([0, 14]);
        container(linha).height(26).align_y(Align::Center).width(Length::FILL).style(estilo_rodape).into()
    }
}

impl App {
    /// O menu do botão direito, no ponto do clique, sem passar das bordas da janela.
    fn view_menu<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some((menu, p)) = &self.menu else { return base };
        let item = |icone: SvgView, rotulo: &str, msg: Option<Msg>| -> Element<'a, Msg> {
            let linha = row![icone.color(COR_TEXTO_MUTED()), text(rotulo.to_owned()).size(13).width(Length::FILL)].spacing(10).align(Align::Center);
            button(linha).width(Length::FILL).padding([7, 10]).style(estilo_botao_pequeno(false)).on_press_maybe(msg.map(|m| Msg::DoMenu(Box::new(m)))).into()
        };
        let traco = |n: &str| feather_ui::icones::icone_traco(n, 16.0);
        let separador = || container(container(Space::new(Length::FILL, 1)).style(|_| Style::default().background(COR_BORDA_SUAVE()))).padding([4, 8]).width(Length::FILL);
        let mut lista = column![].spacing(1);
        match menu {
            Menu::Conexao(id) => {
                let id = *id;
                let Some(i) = self.itens.iter().find(|i| i.id == id) else { return base };
                let ligada = i.principal.banco.is_some();
                lista = lista
                    .push(if ligada { item(icone_tabler(DESCONECTAR, 16.0), tr("Desconectar"), Some(Msg::Desconectar(id))) } else { item(traco("player-play"), tr("Conectar"), Some(Msg::Conectar(id))) })
                    .push(item(traco("refresh"), tr("Atualizar tabelas"), ligada.then_some(Msg::Atualizar((id, None)))))
                    .push(item(traco("file-export"), tr("Exportar o dicionário do banco (CSV)"), (ligada && !i.servidor()).then_some(Msg::ExportarDicionario((id, None)))))
                    .push(separador())
                    .push(item(traco("pencil"), tr("Editar conexão"), Some(Msg::Editar(id))))
                    .push(item(traco("trash"), tr("Excluir conexão"), Some(Msg::Excluir(id))));
            }
            Menu::Banco(id, nome) => {
                let alvo = (*id, Some(nome.clone()));
                let aberto = self.base(&alvo).is_some_and(|b| b.banco.is_some());
                lista = lista
                    .push(item(traco("refresh"), tr("Atualizar tabelas"), aberto.then_some(Msg::Atualizar(alvo.clone()))))
                    .push(item(traco("file-export"), tr("Exportar o dicionário do banco (CSV)"), aberto.then_some(Msg::ExportarDicionario(alvo))));
            }
            Menu::Tabela(alvo, t) => {
                let (id, tipo) = (alvo.clone(), self.tipo(alvo.0));
                let aberta = self.base(alvo).is_some_and(|b| b.tabelas_abertas.contains(&t.qualificado(tipo)));
                let mongo = tipo == Tipo::MongoDb;
                lista = lista
                    .push(item(traco("player-play"), if mongo { tr("Ver os documentos (find)") } else { tr("Ver os dados (SELECT *)") }, Some(Msg::AbrirTabela(id.clone(), t.clone()))))
                    .push(item(traco("columns"), if mongo { tr("Ver os campos") } else { tr("Ver a estrutura") }, Some(Msg::VerEstrutura(id.clone(), t.clone()))))
                    .push(item(traco("pencil"), if mongo { tr("Escrever o find no editor") } else { tr("Escrever o SELECT no editor") }, Some(Msg::EscreverSelect(id.clone(), t.clone()))))
                    .push(item(traco("math-function"), if mongo { tr("Contar os documentos") } else { tr("Contar as linhas (COUNT)") }, Some(Msg::Contar(id.clone(), t.clone()))))
                    .push(separador())
                    .push(item(
                        traco("columns"),
                        match (aberta, mongo) {
                            (true, true) => tr("Esconder os campos"),
                            (false, true) => tr("Mostrar os campos"),
                            (true, false) => tr("Esconder as colunas"),
                            (false, false) => tr("Mostrar as colunas"),
                        },
                        Some(Msg::ExpandirTabela(id.clone(), t.clone())),
                    ))
                    .push(item(traco("copy"), tr("Copiar o nome"), Some(Msg::CopiarTexto(t.qualificado(tipo)))))
                    .push(item(traco("file-export"), if mongo { tr("Exportar os campos (CSV)") } else { tr("Exportar as colunas (CSV)") }, Some(Msg::ExportarColunasDe(id.clone(), t.clone()))));
            }
        }
        const LARGURA: f32 = 270.0;
        const ALTURA: f32 = 300.0;
        let painel = container(lista).width(LARGURA).padding(5).style(|_| {
            Style::default()
                .background(COR_PAINEL())
                .border(Border::rounded(8.0).color(COR_BORDA(), 1.0))
                .shadow(pelicano::Shadow { color: pelicano::Color::BLACK.alpha(0.3), offset: pelicano::Point::new(0.0, 4.0), blur: 14.0 })
        });
        let x = p.x.min(self.tamanho.width - LARGURA - 8.0).max(4.0);
        let y = p.y.min(self.tamanho.height - ALTURA - 8.0).max(4.0);
        let fora = mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(Msg::FecharMenu).on_right_press(Msg::FecharMenu);
        let lugar = container(pelicano::opaque(painel)).padding(pelicano::Padding { top: y, left: x, right: 0.0, bottom: 0.0 });
        pelicano::stack![base, fora, lugar].into()
    }
}

/// Painel com borda e cantos arredondados (o editor e o resultado).
fn painel<'a>(conteudo: impl Into<Element<'a, Msg>>) -> pelicano::Container<'a, Msg> {
    container(conteudo).width(Length::FILL).clip(true).style(|_| Style::default().background(COR_PAINEL()).border(Border::rounded(8.0).color(COR_BORDA_SUAVE(), 1.0)))
}

fn divisoria_vertical<'a>() -> Element<'a, Msg> {
    container(Space::new(1, Length::FILL)).style(|_| Style::default().background(COR_BORDA_SUAVE())).into()
}

fn divisoria<'a>() -> Element<'a, Msg> {
    container(Space::new(Length::FILL, 1)).style(|_| Style::default().background(COR_BORDA_SUAVE())).into()
}

/// Etiqueta pequena arredondada (contagem de linhas, tempo).
fn chip<'a>(texto: String) -> Element<'a, Msg> {
    if texto.is_empty() {
        return Space::new(0, 0).into();
    }
    container(text(texto).size(11.5).color(COR_TEXTO_MUTED())).padding([2, 8]).style(|_| Style::default().background(COR_PAINEL_FAIXA()).border(Border::rounded(9.0).color(COR_BORDA_SUAVE(), 1.0))).into()
}

/// O selo do tipo de banco: duas letras num quadrado com a cor de destaque.
fn selo<'a>(tipo: Tipo) -> Element<'a, Msg> {
    let sigla = match tipo {
        Tipo::Postgres => "PG",
        Tipo::MySql => "MY",
        Tipo::Sqlite => "SQ",
        Tipo::Odbc => "OD",
        Tipo::MongoDb => "MG",
    };
    container(text(sigla).size(10.5).font(Font::BOLD).color(COR_AZUL_ACENTO()))
        .width(28)
        .height(28)
        .align_x(Align::Center)
        .align_y(Align::Center)
        .style(|_| Style::default().background(COR_AZUL_FUNDO()).border(Border::rounded(6.0)))
        .into()
}

/// Botão de ícone menor, para as barras dos painéis.
fn acao_pequena<'a>(icone: SvgView, dica: &str, msg: Option<Msg>) -> Element<'a, Msg> {
    let b = button(icone).padding(6).style(estilo_botao_pequeno(false)).on_press_maybe(msg);
    Element::from(tooltip(b, dica.to_owned(), TooltipPosition::Bottom).padding([4, 8]).style(estilo_dica)).access_label(dica.to_owned())
}

/// O meio do painel vazio: ícone, título e uma dica.
fn vazio<'a>(icone: &str, titulo: &str, dica: &str) -> Element<'a, Msg> {
    let mut c = column![feather_ui::icones::icone_traco(icone, 34.0).color(COR_TEXTO_MUTED()), text(titulo.to_owned()).size(15).font(Font::SEMIBOLD)].spacing(10).align(Align::Center);
    if !dica.is_empty() {
        c = c.push(text(dica.to_owned()).size(12.5).color(COR_TEXTO_MUTED()));
    }
    container(c).width(Length::FILL).height(Length::FILL).align_x(Align::Center).align_y(Align::Center).into()
}

fn main() -> Result<(), pelicano::Error> {
    use feather_ui::novidades;
    let (mut config, existia) = config::Config::carregar();
    // Regra de boas-vindas da suíte: Primeiros passos na 1ª vez, Novidades depois de atualizar.
    let boas_vindas = novidades::ao_abrir(existia, &mut config.primeiros_passos_vistos, &mut config.versao_vista, env!("CARGO_PKG_VERSION"));
    if boas_vindas.is_some() {
        let _ = config.gravar();
    }
    config.aplicar();
    let ajustes = Settings {
        title: "FeatherDB".into(),
        app_id: Some("featherdb".into()),
        size: pelicano::Size::new(1280.0, 800.0),
        min_size: Some(pelicano::Size::new(800.0, 480.0)),
        decorations: false,
        transparent: !cfg!(windows),
        close_on_request: false,
        ..Settings::default()
    };
    pelicano::run(ajustes, move || {
        let mut app = App::novo(config);
        app.janelas.principal = Some(pelicano::WindowId::MAIN);
        let janela = match boas_vindas {
            Some(novidades::Tela::PrimeirosPassos) => Some(Janela::Ajuda),
            Some(novidades::Tela::Novidades) => Some(Janela::Novidades),
            None => None,
        };
        let Some(j) = janela else { return (app, Task::none()) };
        let abrir = app.alternar_janela(j);
        let frente = app.janelas.id(j).map_or(Task::none(), |id| novidades::trazer_para_frente(id, |id| Msg::Aux(Aux::Focar(id))));
        (app, Task::batch([abrir, frente]))
    })
}


#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ctrl_enter_no_comeco_roda_o_primeiro_comando() {
        // Antes do primeiro desenho o cursor do pelicano está em (1, 1): a primeira linha.
        let mut app = App::novo(config::Config::default());
        app.editor = Content::with_text("delete from b;\nselect 1");
        assert_eq!(app.editor.cursor(), (1, 1));
        assert_eq!(app.comando_atual(), "delete from b");
    }
}


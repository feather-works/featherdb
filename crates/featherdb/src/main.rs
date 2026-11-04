//! FeatherDB: cliente de banco de dados leve. À esquerda as conexões com as tabelas; à direita o
//! editor de SQL e a grade do resultado. Postgres, MySQL, SQLite e qualquer banco com ODBC.
mod ajuda;
mod auxiliares;
mod config;
mod formulario;
mod grade;
mod idioma;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use feather_ui::cromo::{self, AcoesJanela};
use feather_ui::icones::icone;
use feather_ui::tema::*;
use featherdb_core::{Banco, Conexao, Resultado, Tabela, Tipo, LIMITE_LINHAS};
use pelicano::{
    button, column, container, horizontal_space, mouse_area, row, svg, text, text_editor, tooltip, Align, Content,
    Element, Event, Font, Key, Length, Modifiers, NamedKey, ResizeDirection, Settings, Space, Style, Svg, SvgView, Task, Theme,
    TooltipPosition, WindowCmd,
};

use auxiliares::{Aux, Janela};
use formulario::{Campo, Formulario};
use idioma::{tr, trf};

/// Cor da marca do FeatherDB (pena azul-petróleo).
const COR_MARCA: &str = "#0b7285";
const LARGURA_LATERAL: f32 = 270.0;

type Aberto = Arc<Mutex<Box<dyn Banco>>>;

/// Uma conexão da lista, com o que está aberto dela nesta sessão.
struct Item {
    id: u64,
    conexao: Conexao,
    banco: Option<Aberto>,
    /// A senha digitada nesta sessão (nunca vai para o disco).
    senha: Option<String>,
    conectando: bool,
    expandido: bool,
    tabelas: Vec<Tabela>,
    /// Colunas das tabelas abertas na árvore, pelo nome qualificado.
    colunas: HashMap<String, Vec<(String, String)>>,
    tabelas_abertas: HashSet<String>,
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
    Form(Campo),
    Senha(String),
    ConfirmarSenha(bool),
    Conectado(u64, Result<Aberto, String>),
    Desconectar(u64),
    Expandir(u64),
    Atualizar(u64),
    Tabelas(u64, Result<Vec<Tabela>, String>),
    ExpandirTabela(u64, Tabela),
    Colunas(u64, String, Result<Vec<(String, String)>, String>),
    AbrirTabela(u64, Tabela),
    // Consultas
    Executar,
    ExecutarTudo,
    Executado(Result<Resultado, String>, Duration),
    Pagina(isize),
    Copiar,
    ExportarCsv,
    Exportado(Option<Result<std::path::PathBuf, String>>),
}

struct App {
    itens: Vec<Item>,
    proximo_id: u64,
    ativa: Option<u64>,
    editor: Content,
    resultado: Option<Resultado>,
    erro: Option<String>,
    executando: bool,
    tempo: Option<Duration>,
    pagina: usize,
    aviso: String,
    form: Option<Formulario>,
    /// Pedindo a senha para conectar: a conexão e o que já foi digitado.
    pedir_senha: Option<(u64, String)>,
    excluir: Option<u64>,
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
const BANCO: &str = "M12 6m-8 0a8 3 0 1 0 16 0a8 3 0 1 0 -16 0M4 6v6a8 3 0 0 0 16 0v-6M4 12v6a8 3 0 0 0 16 0v-6";
const VISAO: &str = "M10 12a2 2 0 1 0 4 0a2 2 0 0 0 -4 0M21 12c-2.4 4 -5.4 6 -9 6c-3.6 0 -6.6 -2 -9 -6c2.4 -4 5.4 -6 9 -6c3.6 0 6.6 2 9 6";
const SETA_DIREITA: &str = "M9 6l6 6l-6 6";
const SETA_BAIXO: &str = "M6 9l6 6l6 -6";
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
            ativa: itens.first().map(|i| i.id),
            itens,
            editor: Content::with_text(&rascunho),
            resultado: None,
            erro: None,
            executando: false,
            tempo: None,
            pagina: 0,
            aviso: String::new(),
            form: None,
            pedir_senha: None,
            excluir: None,
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

    /// Guarda o texto do editor (ao rodar e ao fechar), para nada se perder entre sessões.
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

    /// A senha é pedida para Postgres e MySQL, e no ODBC quando há usuário.
    fn precisa_senha(c: &Conexao) -> bool {
        match c.tipo {
            Tipo::Postgres | Tipo::MySql => true,
            Tipo::Odbc => !c.usuario.is_empty(),
            Tipo::Sqlite => false,
        }
    }

    fn conectar(&mut self, id: u64) -> Task<Msg> {
        let Some(item) = self.item(id) else { return Task::none() };
        if item.banco.is_some() || item.conectando {
            return Task::none();
        }
        if item.senha.is_none() && App::precisa_senha(&item.conexao) {
            self.pedir_senha = Some((id, String::new()));
            return Task::none();
        }
        item.conectando = true;
        let (c, senha) = (item.conexao.clone(), item.senha.clone().unwrap_or_default());
        self.ativa = Some(id);
        em_segundo_plano(move || c.conectar(&senha).map(|b| Arc::new(Mutex::new(b))), move |r| Msg::Conectado(id, r))
    }

    fn carregar_tabelas(&self, id: u64) -> Task<Msg> {
        let Some(banco) = self.itens.iter().find(|i| i.id == id).and_then(|i| i.banco.clone()) else { return Task::none() };
        em_segundo_plano(move || banco.lock().map_err(|_| tr("A conexão travou; desconecte e conecte de novo.").to_owned())?.tabelas(), move |r| Msg::Tabelas(id, r))
    }

    /// Roda os comandos na conexão ativa; fica o resultado do último.
    fn rodar(&mut self, comandos: Vec<String>) -> Task<Msg> {
        if comandos.is_empty() || self.executando {
            return Task::none();
        }
        let Some(item) = self.ativa.and_then(|id| self.itens.iter().find(|i| i.id == id)) else {
            self.erro = Some(tr("Escolha uma conexão à esquerda (ou crie uma em Nova conexão).").into());
            return Task::none();
        };
        let Some(banco) = item.banco.clone() else {
            let id = item.id;
            self.erro = Some(tr("Conecte antes de rodar: clique na conexão à esquerda.").into());
            return self.conectar(id);
        };
        self.executando = true;
        self.erro = None;
        self.gravar_rascunho();
        em_segundo_plano(
            move || {
                let inicio = Instant::now();
                let mut b = match banco.lock() {
                    Ok(b) => b,
                    Err(_) => return (Err(tr("A conexão travou; desconecte e conecte de novo.").to_owned()), inicio.elapsed()),
                };
                let mut r = Ok(Resultado::default());
                for c in &comandos {
                    r = b.executar(c, LIMITE_LINHAS);
                    if r.is_err() {
                        break;
                    }
                }
                (r, inicio.elapsed())
            },
            |(r, t)| Msg::Executado(r, t),
        )
    }
}

impl Item {
    fn novo(id: u64, conexao: Conexao) -> Item {
        Item { id, conexao, banco: None, senha: None, conectando: false, expandido: false, tabelas: Vec::new(), colunas: HashMap::new(), tabelas_abertas: HashSet::new() }
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
                    self.form = Some(Formulario::novo(Some(id), i.conexao.clone()));
                }
            }
            Msg::Excluir(id) => self.excluir = Some(id),
            Msg::ConfirmarExclusao(sim) => {
                if let (true, Some(id)) = (sim, self.excluir) {
                    self.itens.retain(|i| i.id != id);
                    if self.ativa == Some(id) {
                        self.ativa = self.itens.first().map(|i| i.id);
                    }
                    self.gravar_conexoes();
                }
                self.excluir = None;
            }
            Msg::Form(campo) => return self.update_form(campo),
            Msg::Senha(s) => {
                if let Some((_, senha)) = &mut self.pedir_senha {
                    *senha = s;
                }
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
            Msg::Conectado(id, r) => {
                let Some(item) = self.item(id) else { return Task::none() };
                item.conectando = false;
                match r {
                    Ok(b) => {
                        item.banco = Some(b);
                        item.expandido = true;
                        self.erro = None;
                        return self.carregar_tabelas(id);
                    }
                    Err(e) => {
                        // Senha errada: na próxima tentativa pede de novo.
                        item.senha = None;
                        self.erro = Some(trf("Não conectou em {}: {}", &[&item.conexao.rotulo(), &e]));
                    }
                }
            }
            Msg::Desconectar(id) => {
                if let Some(i) = self.item(id) {
                    i.banco = None;
                    i.tabelas.clear();
                    i.colunas.clear();
                    i.tabelas_abertas.clear();
                    i.expandido = false;
                }
            }
            Msg::Expandir(id) => {
                self.ativa = Some(id);
                let Some(i) = self.item(id) else { return Task::none() };
                if i.banco.is_none() {
                    return self.conectar(id);
                }
                i.expandido = !i.expandido;
            }
            Msg::Atualizar(id) => {
                if let Some(i) = self.item(id) {
                    i.colunas.clear();
                    i.tabelas_abertas.clear();
                }
                return self.carregar_tabelas(id);
            }
            Msg::Tabelas(id, r) => match r {
                Ok(t) => {
                    if let Some(i) = self.item(id) {
                        i.tabelas = t;
                    }
                }
                Err(e) => self.erro = Some(trf("Não foi possível ler as tabelas: {}", &[&e])),
            },
            Msg::ExpandirTabela(id, t) => {
                let Some(i) = self.item(id) else { return Task::none() };
                let chave = t.qualificado(i.conexao.tipo);
                if !i.tabelas_abertas.remove(&chave) {
                    i.tabelas_abertas.insert(chave.clone());
                    if !i.colunas.contains_key(&chave) {
                        if let Some(banco) = i.banco.clone() {
                            return em_segundo_plano(
                                move || banco.lock().map_err(|_| tr("A conexão travou; desconecte e conecte de novo.").to_owned())?.colunas(&t),
                                move |r| Msg::Colunas(id, chave, r),
                            );
                        }
                    }
                }
            }
            Msg::Colunas(id, chave, r) => match r {
                Ok(c) => {
                    if let Some(i) = self.item(id) {
                        i.colunas.insert(chave, c);
                    }
                }
                Err(e) => self.erro = Some(e),
            },
            Msg::AbrirTabela(id, t) => {
                self.ativa = Some(id);
                let Some(tipo) = self.itens.iter().find(|i| i.id == id).map(|i| i.conexao.tipo) else { return Task::none() };
                return self.rodar(vec![format!("SELECT * FROM {}", t.qualificado(tipo))]);
            }
            Msg::Executar => {
                let sql = match self.editor.selection().filter(|s| !s.trim().is_empty()) {
                    Some(s) => s,
                    None => {
                        let (l, c) = self.editor.cursor();
                        featherdb_core::comando_no_cursor(&self.editor.text(), l, c)
                    }
                };
                return self.rodar(if sql.trim().is_empty() { Vec::new() } else { vec![sql] });
            }
            Msg::ExecutarTudo => return self.rodar(featherdb_core::dividir(&self.editor.text())),
            Msg::Executado(r, t) => {
                self.executando = false;
                self.tempo = Some(t);
                self.pagina = 0;
                match r {
                    Ok(r) => {
                        self.resultado = Some(r);
                        self.erro = None;
                    }
                    Err(e) => self.erro = Some(e),
                }
            }
            Msg::Pagina(d) => {
                let total = self.resultado.as_ref().map_or(0, |r| grade::paginas(r.linhas.len()));
                self.pagina = (self.pagina as isize + d).clamp(0, total.saturating_sub(1) as isize) as usize;
            }
            Msg::Copiar => {
                if let Some(r) = &self.resultado {
                    let texto = grade::separado(r, '\t');
                    self.aviso = match arboard::Clipboard::new().and_then(|mut c| c.set_text(texto)) {
                        Ok(()) => trf("{} linhas copiadas (cole na planilha).", &[&r.linhas.len()]),
                        Err(e) => trf("Não foi possível copiar: {}", &[&e]),
                    };
                }
            }
            Msg::ExportarCsv => {
                let Some(r) = &self.resultado else { return Task::none() };
                let csv = grade::separado(r, ',');
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
        if self.pedir_senha.is_some() || self.form.is_some() || self.excluir.is_some() {
            return matches!(key, Key::Named(NamedKey::Escape)).then(|| match () {
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
        let corpo = row![self.lateral(), pelicano::vertical_rule(1.0), self.area_consulta()].height(Length::FILL);
        let tudo = column![self.barra(), corpo, self.rodape()];
        let janela = cromo::moldura_redimensionavel_com(tudo.into(), self.maximizada, Msg::Puxar);
        if let Some(f) = &self.form {
            return f.view().sobre(janela);
        }
        if let Some((id, senha)) = &self.pedir_senha {
            let rotulo = self.itens.iter().find(|i| i.id == *id).map(|i| i.conexao.rotulo()).unwrap_or_default();
            let campo = pelicano::text_input(tr("Senha"), senha.as_str()).secure(true).on_input(Msg::Senha).on_submit(Msg::ConfirmarSenha(true)).autofocus(1).padding([7, 10]);
            let corpo = column![text(tr("A senha não é guardada no disco; vale até fechar o FeatherDB.")).size(12.5).color(COR_TEXTO_MUTED()), campo].spacing(12);
            return feather_ui::dialogo::Dialogo::new(trf("Conectar em {}", &[&rotulo]), corpo)
                .icone("lock")
                .largura(420.0)
                .ao_fechar(Msg::ConfirmarSenha(false))
                .secundario(tr("Cancelar"), Some(Msg::ConfirmarSenha(false)))
                .primario(tr("Conectar"), Some(Msg::ConfirmarSenha(true)))
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
fn atalho(key: &Key, m: Modifiers) -> Option<Msg> {
    match key {
        Key::Named(NamedKey::Enter) if m.control_key() && m.shift_key() => Some(Msg::ExecutarTudo),
        Key::Named(NamedKey::Enter) if m.control_key() => Some(Msg::Executar),
        Key::Named(NamedKey::F5) => Some(Msg::ExecutarTudo),
        Key::Character(c) if m.alt_key() && c.eq_ignore_ascii_case("x") => Some(Msg::ExecutarTudo),
        _ => None,
    }
}

impl App {
    fn barra(&self) -> Element<'_, Msg> {
        let pode = !self.executando;
        let barra = row![
            marca(18.0),
            text("FeatherDB").size(14),
            Space::new(12, 0),
            acao("plus", tr("Nova conexão"), Some(Msg::Nova)),
            Space::new(6, 0),
            acao("player-play", tr("Executar o comando do cursor (Ctrl+Enter)"), pode.then_some(Msg::Executar)),
            cromo::acao_icone(icone_tabler(EXECUTAR_TUDO, 18.0), tr("Executar o script inteiro (Alt+X)"), pode.then_some(Msg::ExecutarTudo)),
            Space::new(10, 0),
            text(self.ativa.and_then(|id| self.itens.iter().find(|i| i.id == id)).map(|i| i.conexao.rotulo()).unwrap_or_default()).size(13).color(COR_TEXTO_MUTED()),
            horizontal_space(),
            acao("info-circle", tr("Ajuda (F1)"), Some(Msg::Aux(Aux::Abrir(Janela::Ajuda)))),
            acao("settings", tr("Configurações"), Some(Msg::Aux(Aux::Abrir(Janela::Configuracoes)))),
            Space::new(8, 0),
        ]
        .spacing(4)
        .align(Align::Center);
        let barra = container(barra).padding([0, 12]).align_y(Align::Center).width(Length::FILL).height(cromo::BARRA_ALTURA).style(estilo_container_topo);
        cromo::barra_integrada(
            barra.into(),
            AcoesJanela { arrastar: Msg::Janela(WindowCmd::Drag), minimizar: Msg::Janela(WindowCmd::Minimize), maximizar: Msg::Janela(WindowCmd::ToggleMaximize), fechar: Msg::Fechar },
        )
    }

    /// A árvore: conexões › tabelas › colunas.
    fn lateral(&self) -> Element<'_, Msg> {
        let mut lista = column![].spacing(1);
        if self.itens.is_empty() {
            lista = lista.push(
                column![
                    text(tr("Nenhuma conexão ainda.")).size(13),
                    text(tr("Crie uma para PostgreSQL, MySQL, SQLite ou qualquer banco com driver ODBC (Datasphere, TDV, SQL Server…).")).size(12).color(COR_TEXTO_MUTED()),
                    feather_ui::dialogo::botao_primario(tr("Nova conexão"), Some(Msg::Nova)),
                ]
                .spacing(10)
                .padding(12),
            );
        }
        for item in &self.itens {
            lista = lista.push(self.linha_conexao(item));
            if !item.expandido || item.banco.is_none() {
                continue;
            }
            for t in &item.tabelas {
                let chave = t.qualificado(item.conexao.tipo);
                let aberta = item.tabelas_abertas.contains(&chave);
                let nome = if t.esquema.is_empty() || item.conexao.tipo == Tipo::MySql { t.nome.clone() } else { format!("{}.{}", t.esquema, t.nome) };
                let linha = row![
                    Space::new(18, 0),
                    icone_tabler(if aberta { SETA_BAIXO } else { SETA_DIREITA }, 13.0),
                    if t.visao { icone_tabler(VISAO, 15.0) } else { icone("table", 15.0) },
                    text(nome).size(12.5).no_wrap().ellipsis(),
                ]
                .width(Length::FILL)
                .spacing(5)
                .align(Align::Center);
                let botao = button(linha).padding([3, 6]).width(Length::FILL).style(estilo_botao_pequeno(false)).on_press(Msg::ExpandirTabela(item.id, t.clone()));
                let botao = mouse_area(botao).on_double_click(Msg::AbrirTabela(item.id, t.clone()));
                lista = lista.push(tooltip(botao, tr("Clique duplo mostra os dados"), TooltipPosition::Right).padding([4, 8]).style(estilo_dica));
                if aberta {
                    for (nome, tipo) in item.colunas.get(&chave).map(Vec::as_slice).unwrap_or_default() {
                        lista = lista.push(
                            row![Space::new(56, 0), icone("columns", 13.0), text(nome.as_str()).size(12), text(tipo.as_str()).size(11).color(COR_TEXTO_MUTED()).no_wrap().ellipsis()]
                                .spacing(6)
                                .align(Align::Center)
                                .padding([2, 0]),
                        );
                    }
                }
            }
        }
        // As ações valem para a conexão ativa (a destacada na lista).
        let mut topo = row![text(tr("Conexões")).size(12).color(COR_TEXTO_MUTED()), horizontal_space()].align(Align::Center);
        if let Some(item) = self.ativa.and_then(|id| self.itens.iter().find(|i| i.id == id)) {
            if item.banco.is_some() {
                topo = topo.push(acao("refresh", tr("Atualizar tabelas"), Some(Msg::Atualizar(item.id))));
                topo = topo.push(cromo::acao_icone(icone_tabler(DESCONECTAR, 17.0), tr("Desconectar"), Some(Msg::Desconectar(item.id))));
            }
            topo = topo.push(acao("pencil", tr("Editar conexão"), Some(Msg::Editar(item.id))));
            topo = topo.push(acao("trash", tr("Excluir conexão"), Some(Msg::Excluir(item.id))));
        }
        let topo = topo
            .push(acao("plus", tr("Nova conexão"), Some(Msg::Nova)))
            .padding(pelicano::Padding { top: 4.0, right: 6.0, bottom: 4.0, left: 12.0 });
        container(column![topo, rolagem(container(lista).padding(pelicano::Padding { top: 0.0, right: 6.0, bottom: 8.0, left: 6.0 }))])
            .width(LARGURA_LATERAL)
            .height(Length::FILL)
            .style(|_| Style::default().background(COR_PAINEL_FAIXA()))
            .into()
    }

    fn linha_conexao<'a>(&'a self, item: &'a Item) -> Element<'a, Msg> {
        let ativa = self.ativa == Some(item.id);
        let estado = if item.conectando {
            tr("conectando…")
        } else if item.banco.is_some() {
            ""
        } else {
            tr("desconectada")
        };
        let seta = if item.banco.is_some() && item.expandido { SETA_BAIXO } else { SETA_DIREITA };
        let linha = row![
            icone_tabler(seta, 13.0),
            icone_tabler(BANCO, 16.0),
            column![
                text(item.conexao.rotulo()).size(13).no_wrap().ellipsis(),
                text(if estado.is_empty() { item.conexao.tipo.nome() } else { estado }).size(11).color(COR_TEXTO_MUTED()),
            ]
            .width(Length::FILL),
        ]
        .width(Length::FILL)
        .spacing(6)
        .align(Align::Center);
        button(linha).padding([5, 6]).width(Length::FILL).style(estilo_botao_pequeno(ativa)).on_press(Msg::Expandir(item.id)).into()
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
            .on_key(atalho)
            .colors(move |linha| grade::cores_sql(linha, cor_chave, cor_comentario));
        let editor = container(editor).height(Length::Fill(2)).width(Length::FILL).style(|_| Style::default().background(COR_PAINEL()));
        column![editor, pelicano::horizontal_rule(1.0), container(self.view_resultado()).height(Length::Fill(3)).width(Length::FILL)].width(Length::FILL).into()
    }

    fn view_resultado(&self) -> Element<'_, Msg> {
        if self.executando {
            return centro(text(tr("Rodando…")).size(13).color(COR_TEXTO_MUTED()));
        }
        if let Some(e) = &self.erro {
            let caixa = container(text(e.as_str()).size(13).font(Font::MONO).color(COR_ERRO())).padding(14);
            return rolagem(caixa).into();
        }
        let Some(r) = &self.resultado else {
            return centro(text(tr("O resultado aparece aqui.")).size(13).color(COR_TEXTO_MUTED()));
        };
        if r.colunas.is_empty() {
            let n = r.afetadas.unwrap_or(0);
            return centro(text(trf("Pronto. {} linha(s) afetada(s).", &[&n])).size(13));
        }
        let paginas = grade::paginas(r.linhas.len());
        let barra = row![
            text(if r.cortado { trf("Primeiras {} linhas", &[&r.linhas.len()]) } else { trf("{} linhas", &[&r.linhas.len()]) }).size(12).color(COR_TEXTO_MUTED()),
            horizontal_space(),
            acao("arrow-left", tr("Página anterior"), (self.pagina > 0).then_some(Msg::Pagina(-1))),
            text(trf("{} de {}", &[&(self.pagina + 1), &paginas])).size(12),
            acao("arrow-right", tr("Próxima página"), (self.pagina + 1 < paginas).then_some(Msg::Pagina(1))),
            Space::new(10, 0),
            acao("copy", tr("Copiar tudo (cola na planilha)"), Some(Msg::Copiar)),
            acao("file-export", tr("Exportar CSV"), Some(Msg::ExportarCsv)),
        ]
        .spacing(4)
        .align(Align::Center)
        .padding([2, 10]);
        column![barra, grade::view(r, self.pagina)].into()
    }

    fn rodape(&self) -> Element<'_, Msg> {
        let tempo = self.tempo.map_or(String::new(), |t| trf("{} ms", &[&t.as_millis()]));
        let linha = row![text(self.aviso.as_str()).size(12), horizontal_space(), text(tempo).size(12).color(COR_TEXTO_MUTED())].align(Align::Center).padding([0, 12]);
        container(linha).height(26).align_y(Align::Center).width(Length::FILL).style(estilo_rodape).into()
    }
}

fn centro<'a>(e: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(e).width(Length::FILL).height(Length::FILL).align_x(Align::Center).align_y(Align::Center).into()
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


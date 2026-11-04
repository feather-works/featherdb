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

use feather_ui::tema::*;
use featherdb_core::{Banco, Conexao, Resultado, Tabela, Tipo, LIMITE_LINHAS};
use pelicano::{
    button, column, container, horizontal_space, mouse_area, row, svg, text, text_editor, tooltip, Align, Border, Content,
    Element, Event, Font, Key, Length, Modifiers, NamedKey, ResizeDirection, Settings, Space, Style, Svg, SvgView, Task, Theme,
    TooltipPosition, WindowCmd,
};

use auxiliares::{Aux, Janela};
use formulario::{Campo, Formulario};
use idioma::{tr, trf};

/// Cor da marca do FeatherDB (pena azul-petróleo).
const COR_MARCA: &str = "#0b7285";
const LARGURA_LATERAL: f32 = 270.0;
const MAX_ARVORE: usize = 500;

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
    Filtro(String),
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
    EditorMudou,
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
    /// O texto do filtro da árvore de tabelas.
    filtro: String,
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
            filtro: String::new(),
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
            Msg::Filtro(f) => self.filtro = f,
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
                // A conexão foi editada enquanto conectava: a resposta é da configuração antiga.
                if !item.conectando {
                    return Task::none();
                }
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
                // Cada comando vai sozinho: nem todo driver ODBC aceita vários de uma vez.
                return self.rodar(featherdb_core::dividir(&self.comando_atual()));
            }
            Msg::EditorMudou => self.gravar_rascunho(),
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
        let corpo = row![self.lateral(), divisoria_vertical(), self.area_consulta()].height(Length::FILL);
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
        self.ativa.and_then(|id| self.itens.iter().find(|i| i.id == id))
    }

    /// A árvore: conexões › tabelas › colunas.
    fn lateral(&self) -> Element<'_, Msg> {
        let mut lista = column![].spacing(2);
        if self.itens.is_empty() {
            lista = lista.push(
                column![
                    text(tr("Nenhuma conexão ainda.")).size(13).font(Font::SEMIBOLD),
                    text(tr("Crie uma para PostgreSQL, MySQL, SQLite ou qualquer banco com driver ODBC (Datasphere, TDV, SQL Server…).")).size(12).color(COR_TEXTO_MUTED()).line_height(1.45),
                    feather_ui::dialogo::botao_primario(tr("Nova conexão"), Some(Msg::Nova)),
                ]
                .spacing(10)
                .padding([8, 10]),
            );
        }
        for item in &self.itens {
            lista = lista.push(self.linha_conexao(item));
            if !item.expandido || item.banco.is_none() {
                continue;
            }
            // Bancos como o Datasphere têm milhares de visões: o filtro acha, e a árvore mostra no máximo
            // MAX_ARVORE por conexão para a tela continuar leve.
            let filtro = self.filtro.trim().to_lowercase();
            let achadas: Vec<&Tabela> = item.tabelas.iter().filter(|t| filtro.is_empty() || t.nome.to_lowercase().contains(&filtro) || t.esquema.to_lowercase().contains(&filtro)).collect();
            for t in achadas.iter().take(MAX_ARVORE).copied() {
                let chave = t.qualificado(item.conexao.tipo);
                let aberta = item.tabelas_abertas.contains(&chave);
                let nome = if t.esquema.is_empty() || item.conexao.tipo == Tipo::MySql { t.nome.clone() } else { format!("{}.{}", t.esquema, t.nome) };
                let linha = row![
                    Space::new(14, 0),
                    icone_tabler(if aberta { SETA_BAIXO } else { SETA_DIREITA }, 12.0).color(COR_TEXTO_MUTED()),
                    if t.visao { icone_tabler(VISAO, 15.0).color(COR_TEXTO_MUTED()) } else { feather_ui::icones::icone_traco("table", 15.0).color(COR_TEXTO_MUTED()) },
                    text(nome).size(12.5).no_wrap().ellipsis(),
                ]
                .width(Length::FILL)
                .spacing(6)
                .align(Align::Center);
                let botao = button(linha).padding([4, 6]).width(Length::FILL).style(estilo_botao_pequeno(false)).on_press(Msg::ExpandirTabela(item.id, t.clone()));
                let botao = mouse_area(botao).on_double_click(Msg::AbrirTabela(item.id, t.clone()));
                lista = lista.push(tooltip(botao, tr("Clique duplo mostra os dados"), TooltipPosition::Right).padding([4, 8]).style(estilo_dica));
                if aberta {
                    for (nome, tipo) in item.colunas.get(&chave).map(Vec::as_slice).unwrap_or_default() {
                        lista = lista.push(
                            row![
                                Space::new(48, 0),
                                text(nome.as_str()).size(12).no_wrap().ellipsis(),
                                horizontal_space(),
                                text(tipo.to_lowercase()).size(11).font(Font::MONO).color(COR_TEXTO_MUTED()).no_wrap(),
                            ]
                            .spacing(6)
                            .align(Align::Center)
                            .padding([3, 10]),
                        );
                    }
                }
            }
            if achadas.len() > MAX_ARVORE {
                let resto = achadas.len() - MAX_ARVORE;
                lista = lista.push(container(text(trf("mais {}; use o filtro para achar", &[&resto])).size(12).color(COR_TEXTO_MUTED())).padding([4, 24]));
            }
        }
        // As ações valem para a conexão ativa (a destacada na lista).
        let mut topo = row![text(tr("Conexões")).size(12).font(Font::SEMIBOLD).color(COR_TEXTO_MUTED()), horizontal_space()].spacing(0).align(Align::Center);
        if let Some(item) = self.item_ativo() {
            if item.banco.is_some() {
                topo = topo.push(acao_pequena(feather_ui::icones::icone_traco("refresh", 15.0), tr("Atualizar tabelas"), Some(Msg::Atualizar(item.id))));
                topo = topo.push(acao_pequena(icone_tabler(DESCONECTAR, 15.0), tr("Desconectar"), Some(Msg::Desconectar(item.id))));
            }
            topo = topo.push(acao_pequena(feather_ui::icones::icone_traco("pencil", 15.0), tr("Editar conexão"), Some(Msg::Editar(item.id))));
            topo = topo.push(acao_pequena(feather_ui::icones::icone_traco("trash", 15.0), tr("Excluir conexão"), Some(Msg::Excluir(item.id))));
        }
        let topo = topo
            .push(acao_pequena(feather_ui::icones::icone_traco("plus", 16.0), tr("Nova conexão"), Some(Msg::Nova)))
            .padding(pelicano::Padding { top: 10.0, right: 8.0, bottom: 6.0, left: 14.0 });
        // O filtro só aparece quando há tabelas para filtrar.
        let filtro: Element<'_, Msg> = if self.itens.iter().any(|i| !i.tabelas.is_empty()) {
            container(pelicano::text_input(tr("Filtrar tabelas"), self.filtro.as_str()).on_input(Msg::Filtro).padding([6, 10]).size(12.5)).padding(pelicano::Padding { top: 0.0, right: 10.0, bottom: 8.0, left: 10.0 }).into()
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
        let ativa = self.ativa == Some(item.id);
        let (estado, ligada) = if item.conectando {
            (tr("conectando…"), false)
        } else if item.banco.is_some() {
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
        button(linha).padding([6, 8]).width(Length::FILL).style(estilo_botao_pequeno(ativa)).on_press(Msg::Expandir(item.id)).into()
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
            .on_change(Msg::EditorMudou)
            .colors(move |linha| grade::cores_sql(linha, cor_chave, cor_comentario));
        let editor = painel(column![self.cabecalho_editor(), divisoria(), container(editor).height(Length::FILL).width(Length::FILL)]).height(Length::Fill(2));
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
            Some(i) => row![selo(i.conexao.tipo), text(i.conexao.rotulo()).size(13).font(Font::SEMIBOLD)].spacing(8).align(Align::Center).into(),
            None => text(tr("Nenhuma conexão escolhida")).size(13).color(COR_TEXTO_MUTED()).into(),
        };
        let executar = button(
            row![feather_ui::icones::icone_traco("player-play", 15.0).color(COR_ACENTO_TEXTO(COR_AZUL_ACENTO())), text(tr("Executar")).size(13).font(Font::SEMIBOLD)]
                .spacing(6)
                .align(Align::Center),
        )
        .padding([6, 14])
        .style(estilo_botao_primario)
        .on_press_maybe(pode.then_some(Msg::Executar));
        let script = button(row![icone_tabler(EXECUTAR_TUDO, 15.0), text(tr("Script")).size(13)].spacing(6).align(Align::Center))
            .padding([6, 12])
            .style(estilo_botao_secundario)
            .on_press_maybe(pode.then_some(Msg::ExecutarTudo));
        row![
            onde,
            horizontal_space(),
            tooltip(script, tr("Executar o script inteiro (Alt+X)"), TooltipPosition::Bottom).padding([4, 8]).style(estilo_dica),
            tooltip(executar, tr("Executar o comando do cursor (Ctrl+Enter)"), TooltipPosition::Bottom).padding([4, 8]).style(estilo_dica),
        ]
        .spacing(8)
        .align(Align::Center)
        .padding([8, 12])
        .into()
    }

    fn view_resultado(&self) -> Element<'_, Msg> {
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
        let paginas = grade::paginas(r.linhas.len());
        let contagem = if r.cortado { trf("Primeiras {} linhas", &[&r.linhas.len()]) } else { trf("{} linhas", &[&r.linhas.len()]) };
        let tempo = self.tempo.map_or(String::new(), |t| trf("{} ms", &[&t.as_millis()]));
        let mut barra = row![text(tr("Resultado")).size(13).font(Font::SEMIBOLD), chip(contagem), chip(tempo), horizontal_space()].spacing(8).align(Align::Center);
        if paginas > 1 {
            barra = barra.push(acao_pequena(feather_ui::icones::icone_traco("arrow-left", 15.0), tr("Página anterior"), (self.pagina > 0).then_some(Msg::Pagina(-1))));
            barra = barra.push(text(trf("{} de {}", &[&(self.pagina + 1), &paginas])).size(12).color(COR_TEXTO_MUTED()));
            barra = barra.push(acao_pequena(feather_ui::icones::icone_traco("arrow-right", 15.0), tr("Próxima página"), (self.pagina + 1 < paginas).then_some(Msg::Pagina(1))));
            barra = barra.push(Space::new(6, 0));
        }
        let barra = barra
            .push(acao_pequena(feather_ui::icones::icone_traco("copy", 15.0), tr("Copiar tudo (cola na planilha)"), Some(Msg::Copiar)))
            .push(acao_pequena(feather_ui::icones::icone_traco("file-export", 15.0), tr("Exportar CSV"), Some(Msg::ExportarCsv)))
            .padding([6, 12]);
        column![barra, divisoria(), grade::view(r, self.pagina)].into()
    }

    fn rodape(&self) -> Element<'_, Msg> {
        let estado = match self.item_ativo() {
            Some(i) if i.banco.is_some() => trf("Conectado a {} · {}", &[&i.conexao.rotulo(), &i.conexao.tipo.nome()]),
            Some(i) if i.conectando => trf("Conectando a {}…", &[&i.conexao.rotulo()]),
            _ => tr("Sem conexão").to_owned(),
        };
        let linha = row![text(estado).size(12).color(COR_TEXTO_MUTED()), Space::new(16, 0), text(self.aviso.as_str()).size(12)].align(Align::Center).padding([0, 14]);
        container(linha).height(26).align_y(Align::Center).width(Length::FILL).style(estilo_rodape).into()
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


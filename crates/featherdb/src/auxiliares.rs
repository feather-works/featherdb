//! As janelas auxiliares da suíte: Ajuda (F1), Configurações e Novidades. Cada uma é uma
//! janela própria com a barra da feather-ui (mesmo padrão do FeatherDraw e do FeatherPDF).
use std::time::{Duration, Instant};

use feather_ui::atalhos::{self, PainelGlobal};
use feather_ui::consulta::Ilustracao;
use feather_ui::cromo::{self, AcoesJanela};
use feather_ui::tema::*;
use feather_ui::{ajuda as ui_ajuda, configuracoes, novidades};
use pelicano::{button, column, container, row, text, Align, Border, Element, Key, Length, Modifiers, ResizeDirection, Size, Space, Style, Task, WindowCmd, WindowId};

use crate::config::Tema;
use crate::idioma::{tr, trf, LINGUAS};
use crate::{ajuda, App, Msg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Janela {
    Ajuda,
    Configuracoes,
    Novidades,
}

#[derive(Clone, Debug)]
pub(crate) enum Aux {
    Abrir(Janela),
    Fechar(WindowId),
    Arrastar(WindowId),
    Minimizar(WindowId),
    Maximizar(WindowId),
    Puxar(WindowId, ResizeDirection),
    Focar(WindowId),
    Teclado(WindowId, Key, Modifiers),
    Ajuda(ui_ajuda::Evento),
    Secao(usize),
    Tema(Tema),
    Acento(usize),
    Idioma(&'static str),
    Versao(usize),
    VerNoGithub,
}

/// As versões do FeatherDB, a mais nova primeiro (a tela "Novidades").
pub(crate) static VERSOES: [novidades::Versao; 1] = [novidades::Versao {
    numero: "0.1.0",
    data: "Novembro de 2026",
    resumo: "O primeiro FeatherDB: um cliente de banco de dados leve, que abre na hora.",
    ilustracao: Ilustracao::Planilha,
    destaques: &[
        ("table", "Quatro jeitos de conectar", "PostgreSQL, MySQL/MariaDB e SQLite prontos; Datasphere, TDV e outros por ODBC."),
        ("player-play", "SQL com Ctrl+Enter", "Roda o comando do cursor, a seleção ou o script inteiro, sem travar a janela."),
        ("columns", "Árvore de tabelas", "Tabelas, visões e colunas com o tipo; clique duplo mostra os dados."),
        ("copy", "Leve para a planilha", "Copie o resultado para o FeatherSheet ou o Excel, ou exporte CSV."),
    ],
    tambem: &[
        "A senha fica no cofre de senhas do sistema, nunca num arquivo.",
        "O SQL do editor volta como você deixou na próxima vez.",
        "Interface em português, inglês e espanhol.",
        "Filtro na árvore para bancos com milhares de tabelas.",
        "Menu do botão direito nas tabelas e nas conexões (SELECT, COUNT, editar).",
        "Sem banco escolhido, a árvore lista os bancos do servidor (Postgres e MySQL).",
        "MongoDB e Atlas: coleções na árvore, find, aggregate e countDocuments no editor.",
        "Estrutura da tabela com o tipo de cada coluna; editar e criar coluna mostrando o SQL antes.",
        "Busca de colunas em todos os bancos abertos e exportação do dicionário do banco.",
        "A grade monta só o que aparece na tela: rolar mil linhas é leve.",
        "Cole o endereço postgresql:// ou mysql:// e a conexão se preenche sozinha.",
    ],
}];

const SECOES_CONFIG: [&str; 3] = ["Aparência", "Idioma", "Sobre"];

impl Janela {
    fn tamanho(self) -> Size {
        match self {
            Janela::Ajuda => Size::new(970.0, 700.0),
            Janela::Configuracoes => Size::new(820.0, 560.0),
            Janela::Novidades => Size::new(980.0, 680.0),
        }
    }
    fn titulo(self) -> String {
        match self {
            Janela::Ajuda => tr("Ajuda · FeatherDB").into(),
            Janela::Configuracoes => tr("Configurações · FeatherDB").into(),
            Janela::Novidades => tr("Novidades · FeatherDB").into(),
        }
    }
    fn painel(self) -> PainelGlobal {
        match self {
            Janela::Ajuda => PainelGlobal::Ajuda,
            Janela::Configuracoes | Janela::Novidades => PainelGlobal::Configuracoes,
        }
    }
}

impl App {
    /// Abre a janela (ou fecha, se já estiver aberta). A mesma tecla logo depois é ignorada:
    /// no Windows a janela nova recebe de novo o aperto e o F1 abria e fechava piscando.
    pub(crate) fn alternar_janela(&mut self, j: Janela) -> Task<Msg> {
        let agora = Instant::now();
        if self.janela_alternada.is_some_and(|t| agora.duration_since(t) < Duration::from_millis(500)) {
            return Task::none();
        }
        self.janela_alternada = Some(agora);
        self.janelas.alternar(j, j.tamanho(), true)
    }

    pub(crate) fn update_aux(&mut self, a: Aux) -> Task<Msg> {
        match a {
            Aux::Abrir(j) => return self.alternar_janela(j),
            Aux::Fechar(id) => {
                self.janelas.retirar(id);
                return Task::window_of(id, WindowCmd::Close);
            }
            Aux::Arrastar(id) => return feather_ui::janelas::arrastar_ou_maximizar(id, &mut self.ultimo_clique_titulo),
            Aux::Minimizar(id) => return Task::window_of(id, WindowCmd::Minimize),
            Aux::Maximizar(id) => return Task::window_of(id, WindowCmd::ToggleMaximize),
            Aux::Puxar(id, d) => return Task::window_of(id, WindowCmd::DragResize(d)),
            Aux::Focar(id) => return novidades::focar(id),
            Aux::Teclado(id, key, mods) => {
                if let Some(j) = self.janelas.tipo(id) {
                    if atalhos::fechar_painel(j.painel(), &key, mods) {
                        return self.update_aux(Aux::Fechar(id));
                    }
                }
            }
            Aux::Ajuda(ui_ajuda::Evento::Copiar(texto)) => {
                // O componente só marca o botão como "Copiado"; a cópia de verdade é daqui.
                let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(texto.clone()));
                self.ajuda.atualizar(ui_ajuda::Evento::Copiar(texto), ajuda::catalogo());
            }
            Aux::Ajuda(e) => self.ajuda.atualizar(e, ajuda::catalogo()),
            Aux::Secao(i) => self.secao_config = i,
            Aux::Tema(t) => {
                self.config.tema = t;
                self.gravar_config();
            }
            Aux::Acento(i) => {
                self.config.acento = i;
                self.gravar_config();
            }
            Aux::Idioma(l) => {
                self.config.idioma = l.into();
                self.gravar_config();
            }
            Aux::Versao(i) => self.versao_escolhida = i,
            Aux::VerNoGithub => abrir_no_navegador("https://github.com/feather-works/featherdb"),
        }
        Task::none()
    }

    /// Aplica e grava as preferências (tema, cor e idioma valem na hora, em todas as janelas).
    pub(crate) fn gravar_config(&mut self) {
        self.config.aplicar();
        if let Err(e) = self.config.gravar() {
            self.aviso = trf("Não foi possível guardar as configurações: {}", &[&e]);
        }
    }

    fn acoes(id: WindowId) -> AcoesJanela<Msg> {
        AcoesJanela {
            arrastar: Msg::Aux(Aux::Arrastar(id)),
            minimizar: Msg::Aux(Aux::Minimizar(id)),
            maximizar: Msg::Aux(Aux::Maximizar(id)),
            fechar: Msg::Aux(Aux::Fechar(id)),
        }
    }

    /// O conteúdo de uma janela auxiliar; `None` para a principal.
    pub(crate) fn view_aux(&self, id: WindowId) -> Option<Element<'_, Msg>> {
        let j = self.janelas.tipo(id)?;
        let corpo: Element<'_, Msg> = match j {
            Janela::Ajuda => ajuda::catalogo().view(&self.ajuda).map(|e| Msg::Aux(Aux::Ajuda(e))),
            Janela::Configuracoes => self.view_configuracoes(),
            Janela::Novidades => novidades::tela(
                &VERSOES,
                self.versao_escolhida,
                env!("CARGO_PKG_VERSION"),
                2,
                |s| tr(s).to_owned(),
                novidades::Acoes { escolher: |i| Msg::Aux(Aux::Versao(i)), ver_no_github: Msg::Aux(Aux::VerNoGithub), fechar: Msg::Aux(Aux::Fechar(id)) },
            ),
        };
        let janela = cromo::janela_com_barra(&j.titulo(), corpo, Self::acoes(id));
        Some(cromo::moldura_redimensionavel_com(janela, false, move |d| Msg::Aux(Aux::Puxar(id, d))))
    }

    fn view_configuracoes(&self) -> Element<'_, Msg> {
        let menu = configuracoes::menu(
            tr("Configurações"),
            SECOES_CONFIG.iter().enumerate().map(|(i, s)| (tr(s).to_string(), i == self.secao_config, Msg::Aux(Aux::Secao(i)))),
        );
        let escolha = |rotulo: &str, ativo: bool, msg: Aux| button(text(rotulo.to_owned()).size(12.5)).padding([6, 14]).style(estilo_botao_pequeno(ativo)).on_press(Msg::Aux(msg));
        let secao = self.secao_config.min(SECOES_CONFIG.len() - 1);
        let corpo: Element<'_, Msg> = match secao {
            0 => {
                let t = self.config.tema;
                let mut acentos = row![].spacing(6);
                for (i, (nome, claro, escuro)) in ACENTOS.iter().enumerate() {
                    let cor = pelicano::Color::hex(if feather_ui::tema::escuro() { escuro } else { claro }).unwrap_or(pelicano::Color::BLACK);
                    let bolinha = container(Space::new(20, 20)).style(move |_| Style::default().background(cor).border(Border::rounded(10.0)));
                    acentos = acentos.push(Element::from(button(bolinha).padding(3).style(estilo_botao_pequeno(i == self.config.acento)).on_press(Msg::Aux(Aux::Acento(i)))).access_label(*nome));
                }
                column![
                    configuracoes::linha(tr("Tema"), tr("Automático segue o claro ou escuro do sistema."), row![
                        escolha(tr("Automático"), t == Tema::Automatico, Aux::Tema(Tema::Automatico)),
                        escolha(tr("Claro"), t == Tema::Claro, Aux::Tema(Tema::Claro)),
                        escolha(tr("Escuro"), t == Tema::Escuro, Aux::Tema(Tema::Escuro)),
                    ].spacing(4)),
                    configuracoes::linha(tr("Cor de destaque"), tr("A cor dos botões ativos e das seleções da interface."), acentos),
                ]
                .spacing(18)
                .into()
            }
            1 => {
                let mut linguas = row![].spacing(4);
                for (codigo, nome) in LINGUAS {
                    linguas = linguas.push(escolha(nome, self.config.idioma == codigo, Aux::Idioma(codigo)));
                }
                column![configuracoes::linha(tr("Idioma"), tr("O idioma da interface e da Ajuda."), linguas)].into()
            }
            _ => column![
                row![crate::marca(48.), column![text("FeatherDB").size(20), text(trf("Versão {}", &[&env!("CARGO_PKG_VERSION")])).size(13).color(COR_TEXTO_MUTED())].spacing(2)].spacing(14).align(Align::Center),
                text(tr("Cliente de banco de dados da suíte FeatherWorks. Tudo roda no seu computador: sem conta, sem login, sem enviar nada para fora.")).size(13),
                text(tr("Licença GPL-3.0.")).size(13).color(COR_TEXTO_MUTED()),
                row![
                    feather_ui::dialogo::botao_secundario(tr("O que há de novo"), Some(Msg::Aux(Aux::Abrir(Janela::Novidades)))),
                    feather_ui::dialogo::botao_secundario(tr("Ajuda (F1)"), Some(Msg::Aux(Aux::Abrir(Janela::Ajuda)))),
                ]
                .spacing(8),
            ]
            .spacing(14)
            .into(),
        };
        configuracoes::tela(menu, tr(SECOES_CONFIG[secao]), container(corpo).width(Length::FILL).into(), None)
    }
}

/// Abre o endereço no navegador do sistema, só quando a pessoa clica (nada sai sozinho).
fn abrir_no_navegador(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn novidades_seguem_a_regra_da_suite() {
        feather_ui::novidades::conferir(&VERSOES, env!("CARGO_PKG_VERSION")).unwrap();
    }

    #[test]
    fn novidades_e_secoes_tem_traducao() {
        let mut textos: Vec<&str> = SECOES_CONFIG.to_vec();
        // Os textos da tela de Novidades da feather-ui passam pelo nosso tradutor.
        textos.extend(["Destaques", " · instalada neste computador", "Também nesta versão", "Ver no GitHub", "Versão {}", "Versões"]);
        for v in &VERSOES {
            textos.extend([v.data, v.resumo]);
            textos.extend(v.destaques.iter().flat_map(|(_, t, d)| [*t, *d]));
            textos.extend(v.tambem.iter().copied());
        }
        let faltam: Vec<_> = textos.into_iter().filter(|t| !crate::idioma::sem_traducao(t).is_empty()).collect();
        assert!(faltam.is_empty(), "sem tradução: {faltam:#?}");
    }
}

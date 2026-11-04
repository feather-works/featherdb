//! Várias abas de consulta, cada uma com o seu SQL, a sua conexão e o seu resultado. A aba à
//! mostra mora nos campos do `App`; as outras ficam guardadas em `App::consultas` e trocam de
//! lugar com ela. O texto de cada aba vai para `rascunho.sql`, `rascunho-2.sql`…
use std::time::Duration;

use feather_ui::tema::*;
use featherdb_core::{Resultado, Tabela};
use pelicano::{button, row, text, Align, Content, Element, Font, Length, Space, Task};

use crate::estrutura_ui::Aba;
use crate::idioma::{tr, trf};
use crate::{Alvo, App, Msg};

const MAX_ABAS: usize = 12;

/// Uma aba guardada (fora da tela).
pub(crate) struct Consulta {
    editor: Content,
    ativa: Option<Alvo>,
    resultado: Option<Resultado>,
    erro: Option<String>,
    tempo: Option<Duration>,
    rolagem: (f32, f32),
    versao_resultado: u64,
    tabela_aberta: Option<(Alvo, Tabela)>,
    aba: Aba,
    ddl: Option<(String, Option<Result<String, String>>)>,
    ordenacao: Option<(usize, bool)>,
    filtro_grade: String,
    visiveis: Vec<usize>,
    celulas: crate::celulas::Celulas,
    arquivo_sql: Option<std::path::PathBuf>,
    celula: Option<(usize, usize)>,
}

impl Consulta {
    pub(crate) fn nova(texto: &str, ativa: Option<Alvo>) -> Consulta {
        Consulta {
            editor: Content::with_text(texto),
            ativa,
            resultado: None,
            erro: None,
            tempo: None,
            rolagem: (0.0, 0.0),
            versao_resultado: 0,
            tabela_aberta: None,
            aba: Aba::Dados,
            ddl: None,
            ordenacao: None,
            filtro_grade: String::new(),
            visiveis: Vec::new(),
            celulas: Default::default(),
            arquivo_sql: None,
            celula: None,
        }
    }
}

/// `rascunho.sql` para a primeira aba, `rascunho-N.sql` para as outras.
pub(crate) fn arquivo(i: usize) -> Option<std::path::PathBuf> {
    crate::config::pasta().map(|p| p.join(if i == 0 { "rascunho.sql".to_owned() } else { format!("rascunho-{}.sql", i + 1) }))
}

/// Os textos guardados das abas, em ordem (pelo menos uma).
pub(crate) fn ler_rascunhos() -> Vec<String> {
    let mut v: Vec<String> = (0..MAX_ABAS).map_while(|i| arquivo(i).and_then(|a| std::fs::read_to_string(a).ok())).collect();
    if v.is_empty() {
        v.push(String::new());
    }
    v
}

impl App {
    /// Troca a aba da tela com a guardada em `c` (a da tela vai para `c`).
    fn trocar(&mut self, c: &mut Consulta) {
        std::mem::swap(&mut self.editor, &mut c.editor);
        std::mem::swap(&mut self.ativa, &mut c.ativa);
        std::mem::swap(&mut self.resultado, &mut c.resultado);
        std::mem::swap(&mut self.erro, &mut c.erro);
        std::mem::swap(&mut self.tempo, &mut c.tempo);
        std::mem::swap(&mut self.rolagem, &mut c.rolagem);
        std::mem::swap(&mut self.versao_resultado, &mut c.versao_resultado);
        std::mem::swap(&mut self.tabela_aberta, &mut c.tabela_aberta);
        std::mem::swap(&mut self.aba, &mut c.aba);
        std::mem::swap(&mut self.ddl, &mut c.ddl);
        std::mem::swap(&mut self.ordenacao, &mut c.ordenacao);
        std::mem::swap(&mut self.filtro_grade, &mut c.filtro_grade);
        std::mem::swap(&mut self.visiveis, &mut c.visiveis);
        std::mem::swap(&mut self.celulas, &mut c.celulas);
        std::mem::swap(&mut self.arquivo_sql, &mut c.arquivo_sql);
        std::mem::swap(&mut self.celula, &mut c.celula);
    }

    /// Mostra a aba `i`. O lugar da aba da tela em `consultas` fica com um vazio enquanto ela está à mostra.
    fn ir_para(&mut self, i: usize) {
        if i == self.aba_atual || i >= self.consultas.len() {
            return;
        }
        self.sugestao = None;
        self.gravar_rascunho();
        let mut c = std::mem::replace(&mut self.consultas[i], Consulta::nova("", None));
        self.trocar(&mut c);
        self.consultas[self.aba_atual] = c;
        self.aba_atual = i;
    }

    /// Abre uma aba nova já com o texto (modelo, função, arquivo) e a conexão. Devolve se abriu
    /// (não abre com consulta rodando nem passando do máximo; o aviso diz por quê).
    pub(crate) fn nova_aba_com(&mut self, texto: &str, ativa: Option<Alvo>) -> bool {
        let antes = self.aba_atual;
        let _ = self.update_abas(AcaoAba::Nova);
        if self.aba_atual == antes {
            return false;
        }
        self.editor.set_text(texto);
        if ativa.is_some() {
            self.ativa = ativa;
        }
        self.gravar_rascunho();
        true
    }

    pub(crate) fn update_abas(&mut self, a: AcaoAba) -> Task<Msg> {
        // Com consulta rodando a resposta vai para a aba da tela: só troca depois.
        if self.executando {
            self.aviso = tr("Espere a consulta terminar (ou pare) para trocar de aba.").to_owned();
            return Task::none();
        }
        match a {
            AcaoAba::Ir(i) => self.ir_para(i),
            AcaoAba::Nova => {
                if self.consultas.len() >= MAX_ABAS {
                    self.aviso = trf("No máximo {} abas.", &[&MAX_ABAS]);
                    return Task::none();
                }
                self.consultas.push(Consulta::nova("", self.ativa.clone()));
                self.ir_para(self.consultas.len() - 1);
            }
            AcaoAba::FecharAtual => return self.update_abas(AcaoAba::Fechar(self.aba_atual)),
            AcaoAba::Fechar(i) => {
                if self.consultas.len() == 1 {
                    // A última aba não fecha: só esvazia.
                    let ativa = self.ativa.take();
                    let mut c = Consulta::nova("", ativa);
                    self.trocar(&mut c);
                    self.gravar_rascunho();
                    return Task::none();
                }
                if i == self.aba_atual {
                    self.ir_para(if i + 1 < self.consultas.len() { i + 1 } else { i - 1 });
                }
                self.consultas.remove(i);
                if self.aba_atual > i {
                    self.aba_atual -= 1;
                }
                self.gravar_todos_rascunhos();
            }
        }
        Task::none()
    }

    /// Regrava os arquivos das abas na ordem atual (depois de fechar uma) e apaga o que sobrou.
    fn gravar_todos_rascunhos(&self) {
        for (i, c) in self.consultas.iter().enumerate() {
            let texto = if i == self.aba_atual { self.editor.text() } else { c.editor.text() };
            if let Some(a) = arquivo(i) {
                let _ = std::fs::write(a, texto);
            }
        }
        for i in self.consultas.len()..MAX_ABAS {
            if let Some(a) = arquivo(i) {
                let _ = std::fs::remove_file(a);
            }
        }
    }

    /// A faixa das abas, em cima do editor.
    pub(crate) fn view_abas(&self) -> Element<'_, Msg> {
        let mut r = row![].spacing(2).align(Align::Center);
        for i in 0..self.consultas.len() {
            let atual = i == self.aba_atual;
            let ativa = if atual { self.ativa.as_ref() } else { self.consultas[i].ativa.as_ref() };
            let onde = ativa.and_then(|(id, b)| {
                let i = self.itens.iter().find(|i| i.id == *id)?;
                Some(b.clone().unwrap_or_else(|| i.conexao.rotulo()))
            });
            // Com arquivo .sql, a aba leva o nome dele.
            let arquivo = if atual { self.arquivo_sql.as_ref() } else { self.consultas[i].arquivo_sql.as_ref() };
            let titulo = arquivo.and_then(|a| a.file_name()).map_or_else(|| trf("Consulta {}", &[&(i + 1)]), |n| n.to_string_lossy().into_owned());
            let nome = match onde {
                Some(o) => format!("{titulo} · {o}"),
                None => titulo,
            };
            let fechar = button(feather_ui::icones::icone_traco("x", 12.0).color(COR_TEXTO_MUTED())).padding(2).style(estilo_botao_pequeno(false)).on_press(Msg::Abas(AcaoAba::Fechar(i)));
            let rotulo = button(text(nome).size(12.5).width(Length::Shrink).font(if atual { Font::SEMIBOLD } else { Font::DEFAULT }).no_wrap().ellipsis()).padding([4, 6]).style(estilo_botao_pequeno(atual)).on_press(Msg::Abas(AcaoAba::Ir(i)));
            r = r.push(row![rotulo, fechar].spacing(0).align(Align::Center).width(Length::Shrink));
        }
        let nova = crate::acao_pequena(feather_ui::icones::icone_traco("plus", 14.0), tr("Nova aba de consulta (Ctrl+T)"), Some(Msg::Abas(AcaoAba::Nova)));
        row![r, nova, Space::new(Length::FILL, 0)].spacing(4).align(Align::Center).padding([4, 8]).into()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum AcaoAba {
    Ir(usize),
    Nova,
    Fechar(usize),
    FecharAtual,
}

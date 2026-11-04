//! Completar nomes no editor: ao digitar, sugere tabelas e colunas do banco onde o SQL vai rodar
//! (as que a árvore e o dicionário já leram). Setas escolhem, Tab ou Enter põe, Esc fecha.
use feather_ui::tema::*;
use pelicano::{button, column, container, row, text, Align, Element, Font, Key, Length, Modifiers, NamedKey, Style, Task};

use crate::{App, Msg};

const MAX: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Opcao {
    pub(crate) nome: String,
    /// A tabela da coluna (vazio para tabela).
    pub(crate) de: String,
    pub(crate) tipo: String,
}

pub(crate) struct Sugestao {
    /// O pedaço já digitado (o que a escolha substitui), em bytes da linha antes do cursor.
    prefixo: String,
    opcoes: Vec<Opcao>,
    escolhida: usize,
}

#[derive(Clone, Debug)]
pub(crate) enum Tecla {
    Mover(i32),
    Aceitar(Option<usize>),
    Fechar,
}

/// A palavra antes do cursor e, se ela vem depois de `nome.`, esse nome.
fn palavra(antes: &str) -> (&str, Option<&str>) {
    let ini = antes.char_indices().rev().take_while(|(_, c)| c.is_alphanumeric() || *c == '_').last().map_or(antes.len(), |(i, _)| i);
    let p = &antes[ini..];
    let dono = antes[..ini].strip_suffix('.').map(|r| {
        let r = r.trim_end_matches(['"', '`']);
        let i = r.char_indices().rev().take_while(|(_, c)| c.is_alphanumeric() || *c == '_').last().map_or(r.len(), |(i, _)| i);
        &r[i..]
    });
    (p, dono)
}

/// As opções para o que foi digitado: começam com `prefixo` (sem diferença de maiúscula), sem a que já está completa.
/// Depois de `tabela.`, só as colunas dela (se o nome é de uma tabela conhecida).
pub(crate) fn opcoes(prefixo: &str, dono: Option<&str>, tabelas: &[String], colunas: &[Opcao]) -> Vec<Opcao> {
    let p = prefixo.to_lowercase();
    let serve = |n: &str| {
        let l = n.to_lowercase();
        l.starts_with(&p) && l != p
    };
    let dono = dono.filter(|d| colunas.iter().any(|c| c.de.eq_ignore_ascii_case(d)));
    let mut v: Vec<Opcao> = Vec::new();
    if dono.is_none() {
        v.extend(tabelas.iter().filter(|t| serve(t)).map(|t| Opcao { nome: t.clone(), de: String::new(), tipo: String::new() }));
    }
    for c in colunas.iter().filter(|c| serve(&c.nome) && dono.is_none_or(|d| c.de.eq_ignore_ascii_case(d))) {
        if !v.iter().any(|o| o.nome == c.nome) {
            v.push(c.clone());
        }
        if v.len() > 200 {
            break;
        }
    }
    v.sort_by_key(|o| (o.nome.len(), o.nome.to_lowercase()));
    v.truncate(MAX);
    v
}

impl App {
    /// Depois de digitar: refaz (ou fecha) a lista de sugestões.
    pub(crate) fn sugerir(&mut self) -> Task<Msg> {
        self.sugestao = None;
        if self.editor.selection().is_some() {
            return Task::none();
        }
        let Some(alvo) = self.ativa.clone() else { return Task::none() };
        let tipo = self.tipo(alvo.0);
        if tipo == featherdb_core::Tipo::MongoDb {
            return Task::none();
        }
        let (l, c) = self.editor.cursor();
        let Some(linha) = self.editor.line(l) else { return Task::none() };
        let fim = linha.char_indices().nth(c.saturating_sub(1)).map_or(linha.len(), |(b, _)| b);
        let antes = &linha[..fim];
        let (prefixo, dono) = palavra(antes);
        // Comentário ou texto entre aspas simples: nada a sugerir.
        if antes.contains("--") || antes.matches('\'').count() % 2 == 1 || (prefixo.chars().count() < 2 && dono.is_none()) {
            return Task::none();
        }
        let Some(base) = self.base(&alvo) else { return Task::none() };
        if base.banco.is_none() {
            return Task::none();
        }
        let tabelas: Vec<String> = base.tabelas.iter().map(|t| t.nome.clone()).collect();
        let mut colunas: Vec<Opcao> = match &base.dicionario {
            Some(d) => d.iter().map(|c| Opcao { nome: c.coluna.clone(), de: c.tabela.clone(), tipo: c.tipo.clone() }).collect(),
            None => Vec::new(),
        };
        if base.dicionario.is_none() {
            for t in &base.tabelas {
                for c in base.colunas.get(&t.qualificado(tipo)).map(Vec::as_slice).unwrap_or_default() {
                    colunas.push(Opcao { nome: c.nome.clone(), de: t.nome.clone(), tipo: c.tipo.clone() });
                }
            }
        }
        let opcoes = opcoes(prefixo, dono, &tabelas, &colunas);
        let ler = base.dicionario.is_none() && !base.lendo_dicionario;
        if !opcoes.is_empty() {
            self.sugestao = Some(Sugestao { prefixo: prefixo.to_owned(), opcoes, escolhida: 0 });
        }
        // As colunas de todas as tabelas chegam pelo dicionário (lido uma vez, em segundo plano).
        if ler {
            if let Some(b) = self.base_mut(&alvo) {
                b.lendo_dicionario = true;
            }
            return self.ler_dicionario(alvo, false);
        }
        Task::none()
    }

    pub(crate) fn tecla_sugestao(&mut self, t: Tecla) {
        let Some(s) = &mut self.sugestao else { return };
        match t {
            Tecla::Mover(d) => s.escolhida = (s.escolhida as i32 + d).rem_euclid(s.opcoes.len() as i32) as usize,
            Tecla::Fechar => self.sugestao = None,
            Tecla::Aceitar(i) => {
                let s = self.sugestao.take().expect("sugestão aberta");
                let Some(o) = s.opcoes.get(i.unwrap_or(s.escolhida)) else { return };
                let tipo = self.ativa.as_ref().map_or(featherdb_core::Tipo::Postgres, |a| self.tipo(a.0));
                let nome = featherdb_core::identificador(&o.nome, tipo);
                let (_, c) = self.editor.cursor();
                // Troca o começo da linha até o cursor: o que foi digitado (e uma aspa aberta antes) sai, o nome entra.
                let prefixo = s.prefixo;
                self.editor.edit_lines(move |linha| {
                    let fim = linha.char_indices().nth(c.saturating_sub(1)).map_or(linha.len(), |(b, _)| b);
                    let mut ini = fim.saturating_sub(prefixo.len());
                    if linha[..ini].ends_with(['"', '`']) {
                        ini -= 1;
                    }
                    (fim, format!("{}{nome}", &linha[..ini]))
                });
                self.gravar_rascunho();
            }
        }
    }

    /// As teclas do editor: com a lista aberta, setas, Tab, Enter e Esc são dela.
    pub(crate) fn teclas_editor(&self) -> impl Fn(&Key, Modifiers) -> Option<Msg> + 'static {
        let aberta = self.sugestao.is_some();
        move |k, m| {
            if aberta && !m.control_key() && !m.alt_key() {
                let t = match k {
                    Key::Named(NamedKey::ArrowDown) => Some(Tecla::Mover(1)),
                    Key::Named(NamedKey::ArrowUp) => Some(Tecla::Mover(-1)),
                    Key::Named(NamedKey::Tab | NamedKey::Enter) => Some(Tecla::Aceitar(None)),
                    Key::Named(NamedKey::Escape) => Some(Tecla::Fechar),
                    _ => None,
                };
                if let Some(t) = t {
                    return Some(Msg::Sugestao(t));
                }
            }
            crate::atalho(k, m)
        }
    }

    /// A lista de sugestões, no canto de baixo do editor.
    pub(crate) fn view_sugestao(&self) -> Option<Element<'_, Msg>> {
        let s = self.sugestao.as_ref()?;
        let mut lista = column![].spacing(1);
        for (i, o) in s.opcoes.iter().enumerate() {
            let icone = if o.de.is_empty() { feather_ui::icones::icone_traco("table", 14.0).color(COR_TEXTO_MUTED()) } else { crate::grade::icone_tipo(&o.tipo, false, 14.0) };
            let linha = row![icone, text(o.nome.clone()).size(12.5).font(Font::MONO), pelicano::horizontal_space(), text(o.de.clone()).size(11.5).color(COR_TEXTO_MUTED())]
                .spacing(8)
                .align(Align::Center);
            lista = lista.push(button(linha).padding([4, 8]).width(Length::FILL).style(estilo_botao_pequeno(i == s.escolhida)).on_press(Msg::Sugestao(Tecla::Aceitar(Some(i)))));
        }
        let caixa = container(lista)
            .padding(4)
            .width(340)
            .style(|_| Style::default().background(COR_FUNDO_JANELA()).border(pelicano::Border { color: COR_BORDA(), width: 1.0, radius: [8.0; 4] }));
        Some(container(pelicano::opaque(caixa)).padding(12).width(Length::FILL).height(Length::FILL).align_x(Align::Start).align_y(Align::End).into())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn palavra_antes_do_cursor() {
        assert_eq!(palavra("SELECT * FROM ped"), ("ped", None));
        assert_eq!(palavra("WHERE p.no"), ("no", Some("p")));
        assert_eq!(palavra("WHERE \"Pedidos\".va"), ("va", Some("Pedidos")));
        assert_eq!(palavra("SELECT "), ("", None));
    }

    #[test]
    fn opcoes_por_prefixo_e_pela_tabela() {
        let c = |n: &str, de: &str| Opcao { nome: n.into(), de: de.into(), tipo: "text".into() };
        let tabelas = vec!["pedidos".to_owned(), "pessoas".to_owned()];
        let colunas = vec![c("pedido_id", "itens"), c("nome", "pessoas"), c("nome_fantasia", "lojas")];
        let nomes = |v: Vec<Opcao>| v.into_iter().map(|o| o.nome).collect::<Vec<_>>();
        assert_eq!(nomes(opcoes("PE", None, &tabelas, &colunas)), ["pedidos", "pessoas", "pedido_id"]);
        assert_eq!(nomes(opcoes("no", Some("pessoas"), &tabelas, &colunas)), ["nome"]);
        assert_eq!(nomes(opcoes("n", Some("pessoas"), &tabelas, &colunas)), ["nome"]);
        assert_eq!(nomes(opcoes("nome", None, &tabelas, &colunas)), ["nome_fantasia"]);
    }
}

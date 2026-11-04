//! A visão da tabela: as abas Dados e Estrutura, o diálogo de editar ou criar coluna (que mostra o SQL
//! antes de rodar), a exportação das colunas e do dicionário do banco, e a busca de colunas na lateral.
use feather_ui::tema::*;
use featherdb_core::estrutura::{self, Coluna};
use featherdb_core::{CampoDe, Tabela, Tipo};
use pelicano::{button, checkbox, column, container, row, text, text_input, Align, Element, Font, Length, Space, Style, Task};

use crate::idioma::{tr, trf};
use crate::{acao_pequena, em_segundo_plano, grade, rolagem, vazio, Alvo, App, Msg};

/// O que o painel de baixo mostra quando há uma tabela aberta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Aba {
    Dados,
    Estrutura,
    /// O SQL que cria a tabela (CREATE TABLE / VIEW).
    Sql,
}

/// O diálogo de editar (ou criar) uma coluna.
pub(crate) struct EdicaoColuna {
    alvo: Alvo,
    tabela: Tabela,
    /// `None`: coluna nova.
    original: Option<Coluna>,
    nome: String,
    tipo: String,
    nulo: bool,
    padrao: String,
}

#[derive(Clone, Debug)]
pub(crate) enum CampoColuna {
    Nome(String),
    Tipo(String),
    Nulo(bool),
    Padrao(String),
    Aplicar,
    Cancelar,
}

/// Largura das colunas da lista da estrutura.
const L_NOME: f32 = 230.0;
const L_TIPO: f32 = 230.0;
const L_VAZIO: f32 = 120.0;
const ALTURA: f32 = 34.0;

impl EdicaoColuna {
    fn coluna(&self) -> Coluna {
        let padrao = self.padrao.trim();
        Coluna { nome: self.nome.trim().to_owned(), tipo: self.tipo.trim().to_owned(), nulo: self.nulo, padrao: (!padrao.is_empty()).then(|| padrao.to_owned()), chave: false }
    }

    /// O SQL que vai rodar, ou o motivo de não dar.
    fn sql(&self, tipo: Tipo) -> Result<Vec<String>, String> {
        let tabela = self.tabela.qualificado(tipo);
        match &self.original {
            Some(o) => estrutura::sql_alterar(tipo, &tabela, o, &Coluna { chave: o.chave, ..self.coluna() }),
            None => estrutura::sql_adicionar(tipo, &tabela, &self.coluna()),
        }
    }
}

impl App {
    /// Lê as colunas da tabela (para a aba Estrutura, a árvore ou exportar).
    pub(crate) fn carregar_colunas(&self, alvo: Alvo, t: Tabela, exportar: bool) -> Task<Msg> {
        let Some(banco) = self.base(&alvo).and_then(|b| b.banco.clone()) else { return Task::none() };
        let chave = t.qualificado(self.tipo(alvo.0));
        em_segundo_plano(
            move || crate::travar(&banco)?.colunas(&t),
            move |r| Msg::Colunas(alvo, chave, r, exportar),
        )
    }

    /// Lê o dicionário (todas as colunas) dos bancos abertos que ainda não têm, para a busca.
    pub(crate) fn carregar_dicionarios(&mut self) -> Task<Msg> {
        let mut tarefas = Vec::new();
        let mut alvos = Vec::new();
        for i in &self.itens {
            if !i.servidor() {
                alvos.push((i.id, None));
            }
            alvos.extend(i.bases.keys().map(|n| (i.id, Some(n.clone()))));
        }
        for alvo in alvos {
            if let Some(b) = self.base_mut(&alvo).filter(|b| b.banco.is_some() && b.dicionario.is_none() && !b.lendo_dicionario) {
                b.lendo_dicionario = true;
                tarefas.push(self.ler_dicionario(alvo, false));
            }
        }
        Task::batch(tarefas)
    }

    pub(crate) fn ler_dicionario(&self, alvo: Alvo, exportar: bool) -> Task<Msg> {
        let Some(banco) = self.base(&alvo).and_then(|b| b.banco.clone()) else { return Task::none() };
        em_segundo_plano(
            move || crate::travar(&banco)?.dicionario(),
            move |r| Msg::Dicionario(alvo, r, exportar),
        )
    }

    pub(crate) fn abrir_edicao(&mut self, original: Option<Coluna>) {
        let Some((alvo, tabela)) = self.tabela_aberta.clone() else { return };
        self.edicao = Some(match original {
            Some(c) => EdicaoColuna { alvo, tabela, nome: c.nome.clone(), tipo: c.tipo.clone(), nulo: c.nulo, padrao: c.padrao.clone().unwrap_or_default(), original: Some(c) },
            None => EdicaoColuna { alvo, tabela, original: None, nome: String::new(), tipo: String::new(), nulo: true, padrao: String::new() },
        });
    }

    pub(crate) fn update_edicao(&mut self, campo: CampoColuna) -> Task<Msg> {
        let Some(e) = &mut self.edicao else { return Task::none() };
        match campo {
            CampoColuna::Nome(s) => e.nome = s,
            CampoColuna::Tipo(s) => e.tipo = s,
            CampoColuna::Nulo(v) => e.nulo = v,
            CampoColuna::Padrao(s) => e.padrao = s,
            CampoColuna::Cancelar => self.edicao = None,
            CampoColuna::Aplicar => {
                let id = e.alvo.0;
                let tipo = self.tipo(id);
                let Some(Ok(sql)) = self.edicao.as_ref().map(|e| e.sql(tipo)) else { return Task::none() };
                if sql.is_empty() {
                    self.edicao = None;
                    return Task::none();
                }
                let e = self.edicao.take().expect("edição aberta");
                self.ativa = Some(e.alvo.clone());
                // Depois de rodar, a estrutura é lida de novo para mostrar a coluna como ficou.
                self.recarregar = Some((e.alvo, e.tabela));
                return self.rodar(sql);
            }
        }
        Task::none()
    }

    /// As colunas da tabela aberta, como lista: ícone do tipo, nome, tipo, vazio e padrão.
    pub(crate) fn view_estrutura(&self) -> Element<'_, Msg> {
        let Some((alvo, t)) = &self.tabela_aberta else { return Space::new(0, 0).into() };
        let tipo_banco = self.tipo(alvo.0);
        let Some(colunas) = self.base(alvo).and_then(|b| b.colunas.get(&t.qualificado(tipo_banco))) else {
            return vazio("clock", tr("Lendo as colunas…"), "");
        };
        let editavel = matches!(tipo_banco, Tipo::Postgres | Tipo::MySql | Tipo::Sqlite);
        let celula = |e: Element<'static, Msg>, w: Length| container(e).width(w).height(ALTURA).align_y(Align::Center).padding([0, 8]);
        let rotulo = |s: &str| -> Element<'static, Msg> { text(s.to_owned()).size(12).font(Font::SEMIBOLD).color(COR_TEXTO_MUTED()).into() };
        let cab = row![
            Space::new(34, 0),
            celula(rotulo(tr("Coluna")), Length::Fixed(L_NOME)),
            celula(rotulo(tr("Tipo")), Length::Fixed(L_TIPO)),
            celula(rotulo(tr("Vazio")), Length::Fixed(L_VAZIO)),
            celula(rotulo(tr("Padrão")), Length::FILL),
            Space::new(44, 0),
        ];
        let mut lista = column![container(cab).style(|_| Style::default().background(COR_PAINEL_FAIXA())), crate::divisoria()];
        for (n, c) in colunas.iter().enumerate() {
            let nome: Element<'static, Msg> = row![text(c.nome.clone()).size(13).font(if c.chave { Font::SEMIBOLD } else { Font::DEFAULT }).no_wrap().ellipsis()]
                .push_maybe(c.chave.then(|| container(text(tr("chave")).size(10.5).color(COR_AZUL_ACENTO())).padding([1, 6]).style(|_| Style::default().background(COR_AZUL_FUNDO()).border(pelicano::Border::rounded(8.0)))))
                .spacing(8)
                .align(Align::Center)
                .into();
            let vazio_txt = if c.nulo { tr("aceita vazio") } else { tr("obrigatória") };
            let editar: Element<'static, Msg> = if editavel {
                acao_pequena(feather_ui::icones::icone_traco("pencil", 15.0), tr("Editar coluna"), Some(Msg::EditarColuna(Some(c.clone()))))
            } else {
                Space::new(32, 0).into()
            };
            let linha = row![
                container(grade::icone_tipo(&c.tipo, c.chave, 16.0)).width(34).height(ALTURA).align_x(Align::Center).align_y(Align::Center),
                celula(nome, Length::Fixed(L_NOME)),
                celula(text(c.tipo.clone()).size(12.5).font(Font::MONO).color(COR_TEXTO_MUTED()).no_wrap().ellipsis().into(), Length::Fixed(L_TIPO)),
                celula(text(vazio_txt.to_owned()).size(12.5).color(if c.nulo { COR_TEXTO_MUTED() } else { COR_TEXTO() }).into(), Length::Fixed(L_VAZIO)),
                celula(text(c.padrao.clone().unwrap_or_default()).size(12).font(Font::MONO).color(COR_TEXTO_MUTED()).no_wrap().ellipsis().into(), Length::FILL),
                container(editar).width(44).align_x(Align::Center),
            ]
            .align(Align::Center);
            let par = n % 2 == 1;
            lista = lista.push(container(linha).style(move |_| if par { Style::default().background(COR_PAINEL_FAIXA()) } else { Style::default() }));
        }
        // Embaixo das colunas: índices, chaves estrangeiras e gatilhos (cada um com o SQL dele).
        if let Some(d) = self.base(alvo).and_then(|b| b.detalhes.get(&t.qualificado(tipo_banco))) {
            for (titulo, itens) in [(tr("Índices"), &d.indices), (tr("Chaves estrangeiras"), &d.estrangeiras), (tr("Gatilhos"), &d.gatilhos)] {
                if itens.is_empty() {
                    continue;
                }
                lista = lista.push(Space::new(0, 10)).push(container(text(format!("{titulo} ({})", itens.len())).size(12.5).font(Font::SEMIBOLD).color(COR_TEXTO_MUTED())).padding([4, 12]));
                for (nome, def) in itens {
                    lista = lista.push(
                        row![
                            container(text(nome.clone()).size(12.5).no_wrap().ellipsis()).width(L_NOME).padding([0, 8]),
                            text(def.clone()).size(12).font(Font::MONO).color(COR_TEXTO_MUTED()),
                        ]
                        .padding([4, 26])
                        .align(Align::Center),
                    );
                }
            }
        }
        rolagem(lista).width(Length::FILL).height(Length::FILL).into()
    }

    /// Lê índices, chaves estrangeiras e gatilhos da tabela aberta (uma vez por tabela).
    pub(crate) fn carregar_detalhes(&self) -> Task<Msg> {
        let Some((alvo, t)) = self.tabela_aberta.clone() else { return Task::none() };
        let chave = t.qualificado(self.tipo(alvo.0));
        let Some(banco) = self.base(&alvo).filter(|b| !b.detalhes.contains_key(&chave)).and_then(|b| b.banco.clone()) else { return Task::none() };
        em_segundo_plano(move || crate::travar(&banco)?.detalhes(&t), move |r| Msg::Detalhes(alvo, chave, r))
    }

    /// Lê o CREATE TABLE da tabela aberta (uma vez por tabela).
    pub(crate) fn carregar_ddl(&mut self) -> Task<Msg> {
        let Some((alvo, t)) = self.tabela_aberta.clone() else { return Task::none() };
        let chave = format!("{}/{:?}/{}", alvo.0, alvo.1, t.qualificado(self.tipo(alvo.0)));
        if self.ddl.as_ref().is_some_and(|(c, _)| *c == chave) {
            return Task::none();
        }
        let Some(banco) = self.base(&alvo).and_then(|b| b.banco.clone()) else { return Task::none() };
        self.ddl = Some((chave.clone(), None));
        em_segundo_plano(move || crate::travar(&banco)?.ddl(&t), move |r| Msg::Ddl(chave, r))
    }

    /// A aba SQL: o CREATE da tabela, para copiar ou levar ao editor.
    pub(crate) fn view_ddl(&self) -> Element<'_, Msg> {
        match self.ddl.as_ref().map(|(_, r)| r) {
            Some(Some(Ok(sql))) => rolagem(container(text(sql.as_str()).size(13).font(Font::MONO)).padding(14)).width(Length::FILL).height(Length::FILL).into(),
            Some(Some(Err(e))) => vazio("alert-triangle", tr("Não foi possível ler o SQL da tabela"), e),
            _ => vazio("clock", tr("Lendo o SQL da tabela…"), ""),
        }
    }

    pub(crate) fn acoes_ddl(&self) -> Element<'_, Msg> {
        let pronto = matches!(self.ddl, Some((_, Some(Ok(_)))));
        row![
            acao_pequena(feather_ui::icones::icone_traco("copy", 15.0), tr("Copiar o SQL"), pronto.then_some(Msg::CopiarDdl)),
            acao_pequena(feather_ui::icones::icone_traco("file-code", 15.0), tr("Pôr no editor"), pronto.then_some(Msg::DdlNoEditor)),
        ]
        .spacing(4)
        .into()
    }

    /// As ações da aba Estrutura: adicionar coluna, copiar e exportar a lista.
    pub(crate) fn acoes_estrutura(&self) -> Element<'_, Msg> {
        let Some((alvo, _)) = &self.tabela_aberta else { return Space::new(0, 0).into() };
        let mut r = row![].spacing(4).align(Align::Center);
        if matches!(self.tipo(alvo.0), Tipo::Postgres | Tipo::MySql | Tipo::Sqlite) {
            r = r.push(
                button(row![feather_ui::icones::icone_traco("plus", 14.0), text(tr("Adicionar coluna")).size(12.5)].spacing(6).align(Align::Center))
                    .padding([5, 10])
                    .style(estilo_botao_secundario)
                    .on_press(Msg::EditarColuna(None)),
            );
            r = r.push(Space::new(6, 0));
        }
        r.push(acao_pequena(feather_ui::icones::icone_traco("copy", 15.0), tr("Copiar a lista de colunas (cola na planilha)"), Some(Msg::CopiarColunas)))
            .push(acao_pequena(feather_ui::icones::icone_traco("file-export", 15.0), tr("Exportar as colunas (CSV)"), Some(Msg::ExportarColunas)))
            .into()
    }

    /// O diálogo de editar ou criar coluna, com o SQL que vai rodar embaixo.
    pub(crate) fn view_edicao<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some(e) = &self.edicao else { return base };
        let tipo_banco = self.tipo(e.alvo.0);
        let m = |c: CampoColuna| Msg::Edicao(c);
        let campo = |rotulo: &str, valor: &str, dica: &str, f: fn(String) -> CampoColuna| -> Element<'static, Msg> {
            column![
                text(rotulo.to_owned()).size(12).color(COR_TEXTO_MUTED()),
                text_input(dica.to_owned(), valor.to_owned()).on_input(move |s| Msg::Edicao(f(s))).padding([6, 9]).size(13),
            ]
            .spacing(4)
            .width(Length::FILL)
            .into()
        };
        let mut sugestoes = row![].spacing(4).wrap();
        for s in estrutura::tipos_comuns(tipo_banco) {
            sugestoes = sugestoes.push(
                button(text(*s).size(11.5).font(Font::MONO))
                    .padding([3, 8])
                    .style(estilo_botao_pequeno(e.tipo.trim() == *s))
                    .on_press(m(CampoColuna::Tipo((*s).to_owned()))),
            );
        }
        let sql = e.sql(tipo_banco);
        let previa: Element<'_, Msg> = match &sql {
            Ok(v) if v.is_empty() => text(tr("Nada mudou ainda.")).size(12.5).color(COR_TEXTO_MUTED()).into(),
            Ok(v) => text(v.join(";\n") + ";").size(12).font(Font::MONO).into(),
            Err(err) => text(err.clone()).size(12.5).color(COR_ERRO()).into(),
        };
        let previa = column![
            text(tr("O que vai rodar no banco")).size(12).color(COR_TEXTO_MUTED()),
            container(previa).width(Length::FILL).padding(10).style(|_| Style::default().background(COR_PAINEL_FAIXA()).border(pelicano::Border::rounded(6.0).color(COR_BORDA_SUAVE(), 1.0))),
        ]
        .spacing(4);
        let corpo = column![
            text(trf("Tabela {}", &[&e.tabela.qualificado(tipo_banco)])).size(12.5).color(COR_TEXTO_MUTED()),
            row![campo(tr("Nome"), &e.nome, "", CampoColuna::Nome), campo(tr("Tipo"), &e.tipo, "", CampoColuna::Tipo)].spacing(12),
            sugestoes,
            checkbox(tr("Aceita vazio (NULL)"), e.nulo).on_toggle(|v| Msg::Edicao(CampoColuna::Nulo(v))).size(13),
            campo(tr("Valor padrão (opcional)"), &e.padrao, "0, 'ativo', now()", CampoColuna::Padrao),
            previa,
        ]
        .spacing(12);
        let pode = sql.as_ref().is_ok_and(|v| !v.is_empty());
        let titulo = if e.original.is_some() { tr("Editar coluna") } else { tr("Nova coluna") };
        feather_ui::dialogo::Dialogo::new(titulo, corpo)
            .icone("columns")
            .largura(600.0)
            .ao_fechar(m(CampoColuna::Cancelar))
            .secundario(tr("Cancelar"), Some(m(CampoColuna::Cancelar)))
            .primario(tr("Aplicar no banco"), pode.then(|| m(CampoColuna::Aplicar)))
            .sobre(base)
    }

    /// As colunas que batem com a busca, de todos os bancos abertos (até 40).
    pub(crate) fn resultados_busca(&self) -> Option<Element<'_, Msg>> {
        let termo = self.filtro.trim().to_lowercase();
        if termo.chars().count() < 2 {
            return None;
        }
        let mut achados: Vec<(Alvo, &CampoDe)> = Vec::new();
        let mut lendo = false;
        for i in &self.itens {
            let mut bases: Vec<(Alvo, &crate::Base)> = i.bases.iter().map(|(n, b)| ((i.id, Some(n.clone())), b)).collect();
            if !i.servidor() {
                bases.push(((i.id, None), &i.principal));
            }
            for (alvo, b) in bases {
                lendo |= b.lendo_dicionario;
                for c in b.dicionario.iter().flatten().filter(|c| c.coluna.to_lowercase().contains(&termo)) {
                    achados.push((alvo.clone(), c));
                }
            }
        }
        let mut lista = column![text(tr("Colunas")).size(11.5).font(Font::SEMIBOLD).color(COR_TEXTO_MUTED())].spacing(1);
        if achados.is_empty() {
            lista = lista.push(text(if lendo { tr("procurando nas colunas…") } else { tr("nenhuma coluna com esse nome") }).size(12).color(COR_TEXTO_MUTED()));
        }
        let total = achados.len();
        for (alvo, c) in achados.into_iter().take(40) {
            let linha = row![
                grade::icone_tipo(&c.tipo, false, 14.0),
                text(c.coluna.clone()).size(12.5).no_wrap(),
                text(c.tabela.clone()).size(11.5).color(COR_TEXTO_MUTED()).no_wrap().ellipsis().width(Length::FILL),
            ]
            .spacing(6)
            .align(Align::Center);
            let t = Tabela { esquema: c.esquema.clone(), nome: c.tabela.clone(), ..Default::default() };
            lista = lista.push(button(linha).padding([4, 6]).width(Length::FILL).style(estilo_botao_pequeno(false)).on_press(Msg::VerEstrutura(alvo, t)));
        }
        if total > 40 {
            lista = lista.push(text(trf("mais {}; escreva mais letras", &[&(total - 40)])).size(12).color(COR_TEXTO_MUTED()));
        }
        Some(container(lista).padding(pelicano::Padding { top: 2.0, right: 0.0, bottom: 10.0, left: 6.0 }).into())
    }

    /// Grava o texto num arquivo escolhido pela pessoa.
    pub(crate) fn salvar_texto(nome: String, texto: String) -> Task<Msg> {
        Task::perform(
            async move {
                let arq = rfd::AsyncFileDialog::new().add_filter("CSV", &["csv"]).set_file_name(nome).save_file().await?;
                let caminho = arq.path().to_path_buf();
                Some(std::fs::write(&caminho, texto).map(|_| caminho).map_err(|e| e.to_string()))
            },
            Msg::Exportado,
        )
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn previa_do_sql_da_edicao() {
        let original = Coluna { nome: "nome".into(), tipo: "varchar(50)".into(), nulo: true, padrao: None, chave: false };
        let mut e = EdicaoColuna {
            alvo: (1, None),
            tabela: Tabela { esquema: "public".into(), nome: "cliente".into(), ..Default::default() },
            nome: "nome".into(),
            tipo: "varchar(50)".into(),
            nulo: true,
            padrao: String::new(),
            original: Some(original),
        };
        assert!(e.sql(Tipo::Postgres).unwrap().is_empty());
        e.nulo = false;
        assert_eq!(e.sql(Tipo::Postgres).unwrap(), vec!["ALTER TABLE public.cliente ALTER COLUMN nome SET NOT NULL"]);
        e.original = None;
        e.nome = "email".into();
        assert_eq!(e.sql(Tipo::Postgres).unwrap(), vec!["ALTER TABLE public.cliente ADD COLUMN email varchar(50) NOT NULL"]);
    }
}

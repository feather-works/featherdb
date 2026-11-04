//! Importar CSV para a tabela (botão direito na tabela): escolhe o arquivo, as colunas do CSV casam
//! com as da tabela pelo nome, o diálogo mostra o que entra e o que fica de fora, e os INSERT rodam
//! num bloco só (se um falhar, nada entra).
use feather_ui::tema::*;
use featherdb_core::Tabela;
use pelicano::{column, container, text, Element, Font, Length, Style, Task};

use crate::idioma::{tr, trf};
use crate::{Alvo, App, Msg};

pub(crate) struct Importacao {
    alvo: Alvo,
    tabela: Tabela,
    arquivo: String,
    cabecalho: Vec<String>,
    linhas: Vec<Vec<Option<String>>>,
}

#[derive(Clone, Debug)]
pub(crate) enum AcaoImportar {
    Escolher(Alvo, Tabela),
    Lido(Alvo, Tabela, Option<(String, Result<String, String>)>),
    Confirmar(bool),
}

impl App {
    /// Para cada coluna do CSV, a coluna da tabela com o mesmo nome (sem diferença de maiúscula).
    fn mapa(&self, imp: &Importacao) -> Vec<Option<String>> {
        let tipo = self.tipo(imp.alvo.0);
        let colunas = self.base(&imp.alvo).and_then(|b| b.colunas.get(&imp.tabela.qualificado(tipo)));
        imp.cabecalho.iter().map(|c| colunas.and_then(|cs| cs.iter().find(|x| x.nome.eq_ignore_ascii_case(c))).map(|x| x.nome.clone())).collect()
    }

    pub(crate) fn update_importar(&mut self, a: AcaoImportar) -> Task<Msg> {
        match a {
            AcaoImportar::Escolher(alvo, t) => {
                let tipo = self.tipo(alvo.0);
                // As colunas da tabela são lidas junto, para casar com o cabeçalho do CSV.
                let colunas = if self.base(&alvo).is_some_and(|b| b.colunas.contains_key(&t.qualificado(tipo))) { Task::none() } else { self.carregar_colunas(alvo.clone(), t.clone(), false) };
                let escolher = Task::perform(
                    async {
                        let a = rfd::AsyncFileDialog::new().add_filter("CSV", &["csv", "txt", "tsv"]).pick_file().await?;
                        let nome = a.path().display().to_string();
                        Some((nome, std::fs::read(a.path()).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|e| e.to_string())))
                    },
                    move |r| Msg::Importar(AcaoImportar::Lido(alvo.clone(), t.clone(), r)),
                );
                return Task::batch([colunas, escolher]);
            }
            AcaoImportar::Lido(_, _, None) => {}
            AcaoImportar::Lido(_, _, Some((arquivo, Err(e)))) => self.aviso = trf("Não foi possível abrir {}: {}", &[&arquivo, &e]),
            AcaoImportar::Lido(alvo, tabela, Some((arquivo, Ok(texto)))) => {
                let (cabecalho, linhas) = featherdb_core::importar::ler_csv(&texto);
                if linhas.is_empty() {
                    self.aviso = tr("O arquivo não tem linhas depois do cabeçalho.").to_owned();
                    return Task::none();
                }
                self.importacao = Some(Importacao { alvo, tabela, arquivo, cabecalho, linhas });
            }
            AcaoImportar::Confirmar(sim) => {
                let Some(imp) = self.importacao.take() else { return Task::none() };
                if !sim {
                    return Task::none();
                }
                let tipo = self.tipo(imp.alvo.0);
                let mapa = self.mapa(&imp);
                let inserts = featherdb_core::importar::sql_importar(tipo, &imp.tabela.qualificado(tipo), &mapa, &imp.linhas, 200);
                if inserts.is_empty() {
                    self.aviso = tr("Nenhuma coluna do CSV tem o nome de uma coluna da tabela.").to_owned();
                    return Task::none();
                }
                // Tudo ou nada: BEGIN … COMMIT (se um lote falhar, o ROLLBACK desfaz os anteriores).
                let comandos = std::iter::once("BEGIN".to_owned()).chain(inserts).chain(["COMMIT".to_owned()]).collect();
                self.ativa = Some(imp.alvo.clone());
                self.tabela_aberta = None;
                self.aviso = trf("Importando {} linhas em {}…", &[&imp.linhas.len(), &imp.tabela.nome]);
                return self.rodar(comandos);
            }
        }
        Task::none()
    }

    pub(crate) fn view_importar<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some(imp) = &self.importacao else { return base };
        let mapa = self.mapa(imp);
        let entram: Vec<&str> = mapa.iter().flatten().map(String::as_str).collect();
        let fora: Vec<&str> = imp.cabecalho.iter().zip(&mapa).filter(|(_, m)| m.is_none()).map(|(c, _)| c.as_str()).collect();
        let mut corpo = column![
            text(imp.arquivo.clone()).size(12).font(Font::MONO).color(COR_TEXTO_MUTED()),
            text(trf("{} linhas vão entrar em {}.", &[&imp.linhas.len(), &imp.tabela.nome])).size(13),
            text(trf("Colunas que entram ({}): {}", &[&entram.len(), &entram.join(", ")])).size(12.5),
        ]
        .spacing(8);
        if !fora.is_empty() {
            corpo = corpo.push(text(trf("Ficam de fora (sem coluna com esse nome na tabela): {}", &[&fora.join(", ")])).size(12.5).color(COR_ERRO()));
        }
        corpo = corpo.push(
            container(text(tr("Campo vazio vira NULL. Datas e números precisam estar no formato que o banco aceita. Se uma linha der erro, nenhuma entra.")).size(12).color(COR_TEXTO_MUTED()))
                .padding(10)
                .width(Length::FILL)
                .style(|_| Style::default().background(COR_PAINEL_FAIXA()).border(pelicano::Border::rounded(6.0))),
        );
        let m = |s| Msg::Importar(AcaoImportar::Confirmar(s));
        feather_ui::dialogo::Dialogo::new(tr("Importar CSV"), corpo)
            .icone("file-plus")
            .largura(540.0)
            .ao_fechar(m(false))
            .secundario(tr("Cancelar"), Some(m(false)))
            .primario(tr("Importar"), (!entram.is_empty()).then_some(m(true)))
            .sobre(base)
    }
}

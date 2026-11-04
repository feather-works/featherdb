//! Editar dados na grade: clique duplo numa célula da tabela aberta, Enter guarda a mudança (ainda
//! não no banco), e Salvar mostra os UPDATE antes de rodar. Só em tabela com chave primária
//! (é ela que acha a linha) e em PostgreSQL, MySQL e SQLite.
use std::collections::{BTreeMap, BTreeSet};

use feather_ui::tema::*;
use featherdb_core::{estrutura, Tipo};
use pelicano::{column, container, text, Element, Font, Length, Style, Task};

use crate::idioma::{tr, trf};
use crate::{App, Msg};

#[derive(Default)]
pub(crate) struct Celulas {
    /// (linha, coluna) do resultado → o valor novo (`None` é NULL).
    pub(crate) pendentes: BTreeMap<(usize, usize), Option<String>>,
    /// A célula aberta para digitar e o texto.
    pub(crate) editando: Option<(usize, usize, String)>,
    /// Os UPDATE esperando o "Salvar" do diálogo.
    pub(crate) confirmar: Option<Vec<String>>,
    /// Depois de salvar, ler a tabela de novo.
    pub(crate) recarregar: bool,
    /// Linhas novas (acrescentadas no fim do resultado) e linhas marcadas para excluir.
    pub(crate) novas: BTreeSet<usize>,
    pub(crate) excluir: BTreeSet<usize>,
    /// Quantas linhas o resultado tinha antes das novas (Descartar volta a esse tamanho).
    pub(crate) originais: Option<usize>,
}

impl Celulas {
    pub(crate) fn tem_mudanca(&self) -> bool {
        !self.pendentes.is_empty() || !self.novas.is_empty() || !self.excluir.is_empty()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum AcaoCelula {
    Editar(usize, usize),
    Texto(String),
    Guardar,
    Nulo,
    Cancelar,
    Descartar,
    Salvar,
    NovaLinha,
    /// Marca (ou desmarca) a linha da célula escolhida para excluir.
    ExcluirLinha,
    Confirmar(bool),
}

impl App {
    /// As colunas da chave primária no resultado, quando a grade mostra uma tabela que dá para editar.
    pub(crate) fn chaves_editaveis(&self) -> Option<Vec<usize>> {
        let (alvo, t) = self.tabela_aberta.as_ref()?;
        let tipo = self.tipo(alvo.0);
        if t.visao || !matches!(tipo, Tipo::Postgres | Tipo::MySql | Tipo::Sqlite) || self.executando || self.erro.is_some() {
            return None;
        }
        if self.itens.iter().any(|i| i.id == alvo.0 && i.conexao.somente_leitura) {
            return None;
        }
        let r = self.resultado.as_ref()?;
        let colunas = self.base(alvo)?.colunas.get(&t.qualificado(tipo))?;
        let chaves: Vec<usize> = colunas.iter().filter(|c| c.chave).map(|c| r.colunas.iter().position(|n| *n == c.nome)).collect::<Option<_>>()?;
        (!chaves.is_empty()).then_some(chaves)
    }

    fn guardar_celula(&mut self) {
        let Some((l, c, texto)) = self.celulas.editando.take() else { return };
        let original = self.resultado.as_ref().and_then(|r| r.linhas.get(l)?.get(c).cloned()).flatten();
        if original.as_deref() == Some(texto.as_str()) || (original.is_none() && texto.is_empty()) {
            self.celulas.pendentes.remove(&(l, c));
        } else {
            self.celulas.pendentes.insert((l, c), Some(texto));
        }
    }

    /// Os UPDATE das linhas mudadas (uma por linha).
    fn sql_das_mudancas(&self) -> Result<Vec<String>, String> {
        let (Some(chaves), Some(r), Some((alvo, t))) = (self.chaves_editaveis(), self.resultado.as_ref(), self.tabela_aberta.as_ref()) else {
            return Err(tr("Esta tabela não dá para editar aqui.").to_owned());
        };
        let tipo = self.tipo(alvo.0);
        let tabela = t.qualificado(tipo);
        let ed = &self.celulas;
        let mut por_linha: BTreeMap<usize, Vec<(usize, Option<String>)>> = BTreeMap::new();
        for ((l, c), v) in &ed.pendentes {
            por_linha.entry(*l).or_default().push((*c, v.clone()));
        }
        let sem_chave = |l: usize| trf("A linha {} tem a chave vazia: não dá para achar ela no banco.", &[&(l + 1)]);
        let mut sql = Vec::new();
        // Primeiro os DELETE, depois os UPDATE, por último os INSERT (uma chave apagada pode voltar numa linha nova).
        for &l in ed.excluir.difference(&ed.novas) {
            sql.push(estrutura::sql_excluir(tipo, &tabela, &r.colunas, &chaves, &r.linhas[l]).ok_or_else(|| sem_chave(l))?);
        }
        for (l, m) in &por_linha {
            if ed.excluir.contains(l) || ed.novas.contains(l) {
                continue;
            }
            sql.push(estrutura::sql_atualizar(tipo, &tabela, &r.colunas, &chaves, &r.linhas[*l], m).ok_or_else(|| sem_chave(*l))?);
        }
        for l in ed.novas.difference(&ed.excluir) {
            if let Some(s) = estrutura::sql_inserir(tipo, &tabela, &r.colunas, por_linha.get(l).map(Vec::as_slice).unwrap_or_default()) {
                sql.push(s);
            }
        }
        Ok(sql)
    }

    pub(crate) fn update_celula(&mut self, a: AcaoCelula) -> Task<Msg> {
        match a {
            AcaoCelula::Editar(l, c) => {
                if self.chaves_editaveis().is_none() {
                    self.aviso = tr("Só dá para editar a tabela aberta com clique duplo, e ela precisa de chave primária.").to_owned();
                    return Task::none();
                }
                self.guardar_celula();
                let atual = match self.celulas.pendentes.get(&(l, c)) {
                    Some(v) => v.clone(),
                    None => self.resultado.as_ref().and_then(|r| r.linhas.get(l)?.get(c).cloned()).flatten(),
                };
                self.celulas.editando = Some((l, c, atual.unwrap_or_default()));
            }
            AcaoCelula::Texto(s) => {
                if let Some((_, _, t)) = &mut self.celulas.editando {
                    *t = s;
                }
            }
            AcaoCelula::Guardar => self.guardar_celula(),
            AcaoCelula::Nulo => {
                if let Some((l, c, _)) = self.celulas.editando.take() {
                    let original = self.resultado.as_ref().and_then(|r| r.linhas.get(l)?.get(c).cloned()).flatten();
                    if original.is_none() {
                        self.celulas.pendentes.remove(&(l, c));
                    } else {
                        self.celulas.pendentes.insert((l, c), None);
                    }
                }
            }
            AcaoCelula::Cancelar => self.celulas.editando = None,
            AcaoCelula::Descartar => {
                // As linhas novas saem do resultado.
                if let (Some(n), Some(r)) = (self.celulas.originais, self.resultado.as_mut()) {
                    r.linhas.truncate(n);
                }
                self.celulas = Default::default();
                self.celula = None;
                self.reordenar();
            }
            AcaoCelula::NovaLinha => {
                if self.chaves_editaveis().is_none() {
                    return Task::none();
                }
                self.guardar_celula();
                let Some(r) = self.resultado.as_mut() else { return Task::none() };
                self.celulas.originais.get_or_insert(r.linhas.len());
                r.linhas.push(vec![None; r.colunas.len()]);
                let l = r.linhas.len() - 1;
                self.celulas.novas.insert(l);
                self.reordenar();
                // Já abre a primeira célula para digitar e rola até o fim, onde a linha nova aparece.
                self.celula = Some((l, 0));
                self.celulas.editando = Some((l, 0, String::new()));
                return pelicano::Task::scroll_to(("grade", self.versao_resultado), pelicano::Point::new(self.rolagem.0, f32::INFINITY));
            }
            AcaoCelula::ExcluirLinha => {
                let Some((l, _)) = self.celula else {
                    self.aviso = tr("Clique numa célula da linha que vai sair.").to_owned();
                    return Task::none();
                };
                if self.chaves_editaveis().is_some() && !self.celulas.excluir.remove(&l) {
                    self.celulas.excluir.insert(l);
                }
            }
            AcaoCelula::Salvar => {
                self.guardar_celula();
                match self.sql_das_mudancas() {
                    Ok(sql) if !sql.is_empty() => self.celulas.confirmar = Some(sql),
                    Ok(_) => {}
                    Err(e) => self.aviso = e,
                }
            }
            AcaoCelula::Confirmar(sim) => {
                let Some(sql) = self.celulas.confirmar.take() else { return Task::none() };
                if sim {
                    self.celulas.recarregar = true;
                    // Tudo ou nada: se um UPDATE falhar, os anteriores são desfeitos.
                    let tudo = std::iter::once("BEGIN".to_owned()).chain(sql).chain(["COMMIT".to_owned()]).collect();
                    return self.rodar_conferido(tudo);
                }
            }
        }
        Task::none()
    }

    /// O diálogo com os UPDATE antes de gravar.
    pub(crate) fn view_salvar_celulas<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some(sql) = &self.celulas.confirmar else { return base };
        let texto: String = sql.iter().map(|s| format!("{s};")).collect::<Vec<_>>().join("\n");
        let corpo = column![
            text(trf("{} linha(s) vão mudar no banco:", &[&sql.len()])).size(13),
            container(feather_ui::dialogo::rolagem(text(texto).size(12).font(Font::MONO)))
                .padding(10)
                .width(Length::FILL)
                .max_height(260.0)
                .style(|_| Style::default().background(COR_PAINEL_FAIXA()).border(pelicano::Border::rounded(6.0))),
        ]
        .spacing(10);
        let m = |a| Msg::Celula(a);
        feather_ui::dialogo::Dialogo::new(tr("Salvar as mudanças?"), corpo)
            .icone("device-floppy")
            .largura(560.0)
            .ao_fechar(m(AcaoCelula::Confirmar(false)))
            .secundario(tr("Cancelar"), Some(m(AcaoCelula::Confirmar(false))))
            .primario(tr("Salvar no banco"), Some(m(AcaoCelula::Confirmar(true))))
            .sobre(base)
    }
}

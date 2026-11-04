//! Além das tabelas: as funções do banco na árvore (clique abre o código numa aba), os modelos de
//! view, view materializada e função, e abrir/salvar o SQL da aba num arquivo `.sql`.
use std::path::PathBuf;

use feather_ui::tema::*;
use featherdb_core::{Funcao, Tipo};
use pelicano::{button, container, row, text, tooltip, Align, Element, Font, Length, Space, Task, TooltipPosition};

use crate::idioma::{tr, trf};
use crate::{em_segundo_plano, travar, Alvo, App, Msg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Modelo {
    View,
    Materializada,
    Funcao,
}

#[derive(Clone, Debug)]
pub(crate) enum AcaoObjeto {
    Funcoes(Alvo, Result<Vec<Funcao>, String>),
    MostrarFuncoes(Alvo),
    AbrirFuncao(Alvo, Funcao),
    Definicao(Alvo, Result<String, String>),
    Modelo(Alvo, Modelo),
    Atualizar(Alvo, featherdb_core::Tabela),
    Abrir,
    Aberto(Option<(PathBuf, Result<String, String>)>),
    Salvar,
    SalvarComo,
    Salvo(Option<Result<PathBuf, String>>),
}

/// Bancos que listam funções e têm view materializada.
pub(crate) fn tem_funcoes(t: Tipo) -> bool {
    matches!(t, Tipo::Postgres | Tipo::MySql)
}

/// O texto de um modelo. A view usa o SELECT do editor quando o comando do cursor é uma consulta.
pub(crate) fn modelo(m: Modelo, tipo: Tipo, consulta: Option<&str>) -> String {
    let select = consulta.map_or("SELECT coluna1, coluna2\nFROM tabela\nWHERE condicao".to_owned(), |c| c.trim().trim_end_matches(';').to_owned());
    match (m, tipo) {
        (Modelo::View, _) => format!("CREATE VIEW nome_da_view AS\n{select};\n"),
        (Modelo::Materializada, _) => format!(
            "-- Guarda o resultado: ler é rápido, mas os dados só mudam com o REFRESH.\nCREATE MATERIALIZED VIEW nome_da_view AS\n{select};\n\n-- Para atualizar os dados depois:\n-- REFRESH MATERIALIZED VIEW nome_da_view;\n"
        ),
        (Modelo::Funcao, Tipo::MySql) => {
            "CREATE FUNCTION nome_da_funcao(p_id INT)\nRETURNS VARCHAR(100) DETERMINISTIC\nRETURN CONCAT('id ', p_id);\n\n-- Para usar:\n-- SELECT nome_da_funcao(1);\n".to_owned()
        }
        (Modelo::Funcao, _) => "CREATE OR REPLACE FUNCTION nome_da_funcao(p_id integer)\nRETURNS text\nLANGUAGE plpgsql\nAS $$\nBEGIN\n  RETURN 'id ' || p_id;\nEND;\n$$;\n\n-- Para usar:\n-- SELECT nome_da_funcao(1);\n".to_owned(),
    }
}

impl App {
    /// Lê as funções do banco (depois das tabelas), só onde o banco lista.
    pub(crate) fn carregar_funcoes(&self, alvo: Alvo) -> Task<Msg> {
        if !tem_funcoes(self.tipo(alvo.0)) {
            return Task::none();
        }
        let Some(banco) = self.base(&alvo).and_then(|b| b.banco.clone()) else { return Task::none() };
        em_segundo_plano(move || travar(&banco)?.funcoes(), move |r| Msg::Objeto(AcaoObjeto::Funcoes(alvo, r)))
    }

    pub(crate) fn update_objeto(&mut self, a: AcaoObjeto) -> Task<Msg> {
        match a {
            AcaoObjeto::Funcoes(alvo, r) => {
                if let Some(b) = self.base_mut(&alvo) {
                    // Sem permissão para o catálogo de funções não é erro para mostrar: a árvore só fica sem elas.
                    b.funcoes = r.unwrap_or_default();
                }
            }
            AcaoObjeto::MostrarFuncoes(alvo) => {
                if let Some(b) = self.base_mut(&alvo) {
                    b.funcoes_abertas = !b.funcoes_abertas;
                }
            }
            AcaoObjeto::AbrirFuncao(alvo, f) => {
                let Some(banco) = self.base(&alvo).and_then(|b| b.banco.clone()) else { return Task::none() };
                return em_segundo_plano(move || travar(&banco)?.definicao(&f), move |r| Msg::Objeto(AcaoObjeto::Definicao(alvo, r)));
            }
            AcaoObjeto::Definicao(alvo, r) => match r {
                Ok(sql) => {
                    self.nova_aba_com(&sql, Some(alvo));
                }
                Err(e) => self.aviso = trf("Não foi possível ler a função: {}", &[&e]),
            },
            AcaoObjeto::Modelo(alvo, m) => {
                let consulta = self.comando_atual();
                let consulta = (featherdb_core::so_le(&consulta) && !consulta.trim_start().to_lowercase().starts_with("explain")).then_some(consulta);
                let texto = modelo(m, self.tipo(alvo.0), consulta.as_deref());
                self.nova_aba_com(&texto, Some(alvo));
            }
            AcaoObjeto::Atualizar(alvo, t) => {
                self.ativa = Some(alvo.clone());
                self.tabela_aberta = None;
                return self.rodar(vec![format!("REFRESH MATERIALIZED VIEW {}", t.qualificado(Tipo::Postgres))]);
            }
            AcaoObjeto::Abrir => {
                return Task::perform(
                    async {
                        let a = rfd::AsyncFileDialog::new().add_filter("SQL", &["sql", "txt"]).add_filter("*", &["*"]).pick_file().await?;
                        let caminho = a.path().to_path_buf();
                        let texto = std::fs::read(&caminho).map(|b| String::from_utf8_lossy(&b).into_owned()).map_err(|e| e.to_string());
                        Some((caminho, texto))
                    },
                    |r| Msg::Objeto(AcaoObjeto::Aberto(r)),
                );
            }
            AcaoObjeto::Aberto(None) => {}
            AcaoObjeto::Aberto(Some((caminho, Err(e)))) => self.aviso = trf("Não foi possível abrir {}: {}", &[&caminho.display(), &e]),
            AcaoObjeto::Aberto(Some((caminho, Ok(texto)))) => {
                // Aba vazia recebe o arquivo; senão ele abre numa aba nova.
                if self.editor.text().trim().is_empty() && self.arquivo_sql.is_none() {
                    self.editor.set_text(&texto);
                    self.gravar_rascunho();
                } else if !self.nova_aba_com(&texto, None) {
                    return Task::none();
                }
                self.arquivo_sql = Some(caminho);
            }
            AcaoObjeto::Salvar if self.arquivo_sql.is_some() => {
                let caminho = self.arquivo_sql.clone().expect("arquivo da aba");
                let r = std::fs::write(&caminho, self.editor.text()).map(|_| caminho).map_err(|e| e.to_string());
                return self.update_objeto(AcaoObjeto::Salvo(Some(r)));
            }
            AcaoObjeto::Salvar | AcaoObjeto::SalvarComo => {
                let texto = self.editor.text();
                let nome = self.arquivo_sql.as_ref().and_then(|c| c.file_name()).map_or("consulta.sql".to_owned(), |n| n.to_string_lossy().into_owned());
                return Task::perform(
                    async move {
                        let a = rfd::AsyncFileDialog::new().add_filter("SQL", &["sql"]).set_file_name(nome).save_file().await?;
                        let caminho = a.path().to_path_buf();
                        Some(std::fs::write(&caminho, texto).map(|_| caminho).map_err(|e| e.to_string()))
                    },
                    |r| Msg::Objeto(AcaoObjeto::Salvo(r)),
                );
            }
            AcaoObjeto::Salvo(None) => {}
            AcaoObjeto::Salvo(Some(Ok(c))) => {
                self.aviso = trf("Salvo em {}", &[&c.display()]);
                self.arquivo_sql = Some(c);
            }
            AcaoObjeto::Salvo(Some(Err(e))) => self.aviso = trf("Não foi possível salvar: {}", &[&e]),
        }
        Task::none()
    }

    /// As funções de um banco na árvore: um título que abre e fecha, e cada função embaixo.
    pub(crate) fn arvore_funcoes<'a>(&'a self, alvo: &Alvo, funcoes: &'a [Funcao], abertas: bool, recuo: f32) -> Vec<Element<'a, Msg>> {
        let filtro = self.filtro.trim().to_lowercase();
        let achadas: Vec<&Funcao> = funcoes.iter().filter(|f| filtro.is_empty() || f.nome.to_lowercase().contains(&filtro)).collect();
        if achadas.is_empty() {
            return Vec::new();
        }
        let seta = if abertas { crate::SETA_BAIXO } else { crate::SETA_DIREITA };
        let titulo = row![
            Space::new(recuo + 14.0, 0),
            crate::icone_tabler(seta, 12.0).color(COR_TEXTO_MUTED()),
            feather_ui::icones::icone_traco("math-function", 15.0).color(COR_TEXTO_MUTED()),
            text(trf("Funções ({})", &[&achadas.len()])).size(12.5).font(Font::SEMIBOLD).color(COR_TEXTO_MUTED()),
        ]
        .width(Length::FILL)
        .spacing(6)
        .align(Align::Center);
        let mut saida: Vec<Element<'a, Msg>> =
            vec![button(titulo).padding([4, 6]).width(Length::FILL).style(estilo_botao_pequeno(false)).on_press(Msg::Objeto(AcaoObjeto::MostrarFuncoes(alvo.clone()))).into()];
        if abertas {
            for f in achadas.into_iter().take(crate::MAX_ARVORE) {
                let linha = row![
                    Space::new(recuo + 40.0, 0),
                    text(f.nome.clone()).size(12.5).no_wrap(),
                    text(format!("({})", f.argumentos)).size(11.5).font(Font::MONO).color(COR_TEXTO_MUTED()).no_wrap().ellipsis(),
                ]
                .width(Length::FILL)
                .spacing(4)
                .align(Align::Center);
                let botao = button(linha).padding([3, 6]).width(Length::FILL).style(estilo_botao_pequeno(false)).on_press(Msg::Objeto(AcaoObjeto::AbrirFuncao(alvo.clone(), f.clone())));
                saida.push(tooltip(botao, tr("Abre o código numa aba nova"), TooltipPosition::Right).padding([4, 8]).style(estilo_dica).into());
            }
        }
        saida
    }

    /// Os botões de abrir e salvar `.sql` da faixa do editor.
    pub(crate) fn acoes_arquivo(&self) -> Element<'_, Msg> {
        let abrir = crate::acao_pequena(feather_ui::icones::icone_traco("folder-open", 15.0), tr("Abrir arquivo .sql (Ctrl+O)"), Some(Msg::Objeto(AcaoObjeto::Abrir)));
        let salvar = crate::acao_pequena(feather_ui::icones::icone_traco("device-floppy", 15.0), tr("Salvar em arquivo .sql (Ctrl+S)"), Some(Msg::Objeto(AcaoObjeto::Salvar)));
        container(row![abrir, salvar].spacing(2)).into()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn modelos_usam_a_consulta_do_editor() {
        let v = modelo(Modelo::View, Tipo::Postgres, Some("select * from t;"));
        assert_eq!(v, "CREATE VIEW nome_da_view AS\nselect * from t;\n");
        assert!(modelo(Modelo::Materializada, Tipo::Postgres, None).contains("CREATE MATERIALIZED VIEW nome_da_view AS\nSELECT coluna1"));
        // A função do Postgres tem o corpo entre $$: o `;` de dentro não corta o comando.
        assert!(featherdb_core::dividir(&modelo(Modelo::Funcao, Tipo::Postgres, None))[0].ends_with("$$"));
        assert!(featherdb_core::dividir(&modelo(Modelo::Funcao, Tipo::MySql, None))[0].ends_with("p_id)"));
    }
}

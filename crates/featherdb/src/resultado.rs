//! Em volta da grade: a célula escolhida e o painel de valor (o valor inteiro, JSON arrumado e as
//! contas da coluna), copiar como INSERT/JSON/Markdown, exportar no formato da extensão do arquivo
//! e buscar mais linhas.
use std::collections::HashSet;
use std::path::PathBuf;

use feather_ui::tema::*;
use featherdb_core::estrutura::{familia, Familia};
use featherdb_core::{Resultado, Tipo};
use pelicano::{button, column, container, row, text, Align, Element, Font, Length, Space, Style, Task};

use crate::idioma::{tr, trf};
use crate::{grade, App, Msg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Formato {
    Planilha,
    Insert,
    Json,
    Markdown,
}

/// O texto das linhas no formato (o INSERT usa a tabela aberta, ou `tabela` quando é uma consulta solta).
pub(crate) fn texto(r: &Resultado, linhas: &[usize], f: Formato, tabela: &str, tipo: Tipo) -> String {
    match f {
        Formato::Planilha => grade::separado(r, linhas, '\t'),
        Formato::Insert => grade::como_insert(r, linhas, tabela, tipo),
        Formato::Json => grade::como_json(r, linhas),
        Formato::Markdown => grade::como_markdown(r, linhas),
    }
}

/// As contas de uma coluna (só as linhas à mostra): quantas, vazias, diferentes e, se for número, soma, média, mínimo e máximo.
pub(crate) fn contas(r: &Resultado, linhas: &[usize], c: usize) -> Vec<(String, String)> {
    let valores: Vec<&str> = linhas.iter().filter_map(|&i| r.linhas[i].get(c).and_then(|v| v.as_deref())).collect();
    let mut s = vec![
        (tr("Linhas").to_owned(), linhas.len().to_string()),
        (tr("Vazias (NULL)").to_owned(), (linhas.len() - valores.len()).to_string()),
        (tr("Diferentes").to_owned(), valores.iter().collect::<HashSet<_>>().len().to_string()),
    ];
    let numeros: Vec<f64> = valores.iter().filter_map(|v| v.trim().parse::<f64>().ok()).collect();
    if !numeros.is_empty() && numeros.len() == valores.len() {
        let soma: f64 = numeros.iter().sum();
        let fmt = |x: f64| if x.fract() == 0.0 && x.abs() < 1e15 { format!("{x:.0}") } else { format!("{x:.4}").trim_end_matches('0').trim_end_matches('.').to_owned() };
        s.push((tr("Soma").to_owned(), fmt(soma)));
        s.push((tr("Média").to_owned(), fmt(soma / numeros.len() as f64)));
        s.push((tr("Mínimo").to_owned(), fmt(numeros.iter().copied().fold(f64::INFINITY, f64::min))));
        s.push((tr("Máximo").to_owned(), fmt(numeros.iter().copied().fold(f64::NEG_INFINITY, f64::max))));
    }
    s
}

impl App {
    /// A tabela para o INSERT copiado: a aberta, senão um nome para a pessoa trocar.
    fn tabela_para_insert(&self) -> (String, Tipo) {
        match &self.tabela_aberta {
            Some((alvo, t)) => {
                let tipo = self.tipo(alvo.0);
                (t.qualificado(tipo), tipo)
            }
            None => ("tabela".into(), self.ativa.as_ref().map_or(Tipo::Postgres, |a| self.tipo(a.0))),
        }
    }

    /// Copia (só a linha da célula escolhida, ou tudo o que está à mostra) no formato.
    pub(crate) fn copiar_como(&mut self, f: Formato, so_linha: bool) {
        let Some(r) = &self.resultado else { return };
        let linhas: Vec<usize> = match (so_linha, self.celula) {
            (true, Some((l, _))) => vec![l],
            _ => self.visiveis.clone(),
        };
        let (tabela, tipo) = self.tabela_para_insert();
        let t = texto(r, &linhas, f, &tabela, tipo);
        self.aviso = match arboard::Clipboard::new().and_then(|mut c| c.set_text(t)) {
            Ok(()) => trf("{} linhas copiadas.", &[&linhas.len()]),
            Err(e) => trf("Não foi possível copiar: {}", &[&e]),
        };
    }

    pub(crate) fn copiar_valor(&mut self) {
        let Some(v) = self.celula.and_then(|(l, c)| self.resultado.as_ref()?.linhas.get(l)?.get(c).cloned()) else { return };
        let v = v.unwrap_or_else(|| "NULL".into());
        self.aviso = match arboard::Clipboard::new().and_then(|mut c| c.set_text(v)) {
            Ok(()) => tr("Valor copiado.").to_owned(),
            Err(e) => trf("Não foi possível copiar: {}", &[&e]),
        };
    }

    /// Pergunta onde salvar; o formato sai da extensão escolhida (CSV, JSON, SQL ou Markdown).
    pub(crate) fn exportar(&self) -> Task<Msg> {
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .add_filter("JSON", &["json"])
                    .add_filter("SQL (INSERT)", &["sql"])
                    .add_filter("Markdown", &["md"])
                    .set_file_name("resultado.csv")
                    .save_file()
                    .await
                    .map(|a| a.path().to_path_buf())
            },
            Msg::ExportarPara,
        )
    }

    pub(crate) fn exportar_para(&mut self, caminho: PathBuf) {
        let Some(r) = &self.resultado else { return };
        let ext = caminho.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let (tabela, tipo) = self.tabela_para_insert();
        let conteudo = match ext.as_str() {
            "json" => texto(r, &self.visiveis, Formato::Json, &tabela, tipo),
            "sql" => texto(r, &self.visiveis, Formato::Insert, &tabela, tipo),
            "md" => texto(r, &self.visiveis, Formato::Markdown, &tabela, tipo),
            _ => grade::separado(r, &self.visiveis, ','),
        };
        self.aviso = match std::fs::write(&caminho, conteudo) {
            Ok(()) => trf("Salvo em {}", &[&caminho.display()]),
            Err(e) => trf("Não foi possível salvar: {}", &[&e]),
        };
    }

    /// O painel de valor, à direita da grade: a célula escolhida por inteiro e as contas da coluna.
    pub(crate) fn view_painel_valor(&self) -> Option<Element<'_, Msg>> {
        let (l, c) = self.celula.filter(|_| self.painel_valor)?;
        let r = self.resultado.as_ref()?;
        let nome = r.colunas.get(c)?.clone();
        let tipo = r.tipos.get(c).cloned().unwrap_or_default();
        let v = r.linhas.get(l)?.get(c)?.clone();
        // JSON vira árvore indentada para ler; o resto aparece como veio.
        let mostrar = match &v {
            None => "NULL".to_owned(),
            Some(s) if familia(&tipo) == Familia::Json || s.trim_start().starts_with(['{', '[']) => {
                serde_json::from_str::<serde_json::Value>(s).ok().and_then(|j| serde_json::to_string_pretty(&j).ok()).unwrap_or_else(|| s.clone())
            }
            Some(s) => s.clone(),
        };
        let cab = row![
            grade::icone_tipo(&tipo, false, 15.0),
            column![text(nome).size(13).font(Font::SEMIBOLD), text(tipo).size(11).font(Font::MONO).color(COR_TEXTO_MUTED())].spacing(1),
            pelicano::horizontal_space(),
            crate::acao_pequena(feather_ui::icones::icone_traco("copy", 14.0), tr("Copiar o valor"), Some(Msg::CopiarValor)),
            crate::acao_pequena(feather_ui::icones::icone_traco("x", 14.0), tr("Fechar o painel"), Some(Msg::PainelValor)),
        ]
        .spacing(8)
        .align(Align::Center);
        let valor = container(crate::rolagem(text(mostrar).size(12.5).font(Font::MONO).color(if v.is_none() { COR_TEXTO_MUTED() } else { COR_TEXTO() })))
            .padding(10)
            .width(Length::FILL)
            .height(Length::FILL)
            .style(|_| Style::default().background(COR_PAINEL_FAIXA()).border(pelicano::Border::rounded(6.0)));
        let mut lista = column![text(tr("Coluna (linhas à mostra)")).size(11.5).font(Font::SEMIBOLD).color(COR_TEXTO_MUTED())].spacing(3);
        for (k, val) in contas(r, &self.visiveis, c) {
            lista = lista.push(row![text(k).size(12).color(COR_TEXTO_MUTED()), pelicano::horizontal_space(), text(val).size(12).font(Font::MONO)].align(Align::Center));
        }
        let painel = column![cab, valor, lista].spacing(10).padding(10).width(300);
        Some(row![crate::divisoria_vertical(), painel].into())
    }

    /// O botão de buscar mais linhas, quando o resultado foi cortado no limite.
    pub(crate) fn botao_buscar_mais(&self) -> Option<Element<'_, Msg>> {
        let r = self.resultado.as_ref().filter(|r| r.cortado)?;
        let _ = r;
        let b = button(row![text(trf("+{} linhas", &[&featherdb_core::LIMITE_LINHAS])).size(12)].align(Align::Center))
            .padding([4, 9])
            .style(estilo_botao_secundario)
            .on_press(Msg::BuscarMais);
        let b = pelicano::tooltip(b, tr("Buscar mais linhas"), pelicano::TooltipPosition::Bottom).padding([4, 8]).style(estilo_dica);
        Some(row![b, Space::new(4, 0)].into())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn contas_da_coluna_de_numeros_e_de_texto() {
        let l = |v: Option<&str>| vec![v.map(str::to_owned)];
        let r = Resultado { colunas: vec!["n".into()], linhas: vec![l(Some("2")), l(Some("4")), l(None), l(Some("4"))], ..Default::default() };
        let c = contas(&r, &[0, 1, 2, 3], 0);
        let v: Vec<&str> = c.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(v, ["4", "1", "2", "10", "3.3333", "2", "4"]);
        let t = Resultado { colunas: vec!["n".into()], linhas: vec![l(Some("a")), l(Some("2"))], ..Default::default() };
        assert_eq!(contas(&t, &[0, 1], 0).len(), 3);
    }
}

//! Diagrama ER (só leitura): uma caixa por tabela com as colunas e uma linha de cada chave
//! estrangeira até a tabela que ela aponta. Lido do banco ao abrir (colunas e chaves de cada tabela).
use feather_ui::tema::*;
use featherdb_core::{Coluna, Tabela};
use pelicano::{Border, Element, Font, Point, Rect};

use crate::idioma::{tr, trf};
use crate::{Alvo, App, Msg};

/// Tabelas demais deixam o diagrama ilegível e a leitura lenta (duas consultas por tabela).
pub(crate) const MAX_TABELAS: usize = 80;
const MAX_COLUNAS: usize = 12;
const LARGURA: f32 = 250.0;
const VAO: f32 = 40.0;
const CABECA: f32 = 26.0;
const LINHA: f32 = 18.0;
const POR_LINHA: usize = 3;

#[derive(Clone, Debug)]
pub(crate) struct Caixa {
    pub(crate) nome: String,
    pub(crate) colunas: Vec<Coluna>,
}

#[derive(Clone, Debug)]
pub(crate) struct Diagrama {
    pub(crate) caixas: Vec<Caixa>,
    /// (de, para): índices em `caixas`.
    pub(crate) ligacoes: Vec<(usize, usize)>,
    /// Quantas tabelas ficaram de fora pelo limite.
    pub(crate) de_fora: usize,
}

/// A tabela de `FOREIGN KEY (x) REFERENCES esquema.tabela(id)`, sem esquema nem aspas.
pub(crate) fn referencia(def: &str) -> Option<String> {
    let i = def.to_uppercase().find("REFERENCES ")? + "REFERENCES ".len();
    let resto = def[i..].trim_start();
    let alvo = resto.split(['(', ' ']).next()?;
    let nome = alvo.rsplit('.').next()?.trim_matches(['"', '`', '[', ']']);
    (!nome.is_empty()).then(|| nome.to_owned())
}

/// Lê colunas e chaves estrangeiras de cada tabela (roda fora da janela).
pub(crate) fn ler(banco: &crate::Aberto, tabelas: Vec<Tabela>) -> Result<Diagrama, String> {
    let mut b = crate::travar(banco)?;
    let mut tabelas: Vec<Tabela> = tabelas.into_iter().filter(|t| !t.visao).collect();
    let de_fora = tabelas.len().saturating_sub(MAX_TABELAS);
    tabelas.truncate(MAX_TABELAS);
    let mut caixas = Vec::new();
    let mut chaves = Vec::new();
    for t in &tabelas {
        caixas.push(Caixa { nome: t.nome.clone(), colunas: b.colunas(t)? });
        // Sem permissão para o catálogo de chaves a tabela aparece sem linhas, não é erro.
        chaves.push(b.detalhes(t).map(|d| d.estrangeiras).unwrap_or_default());
    }
    let mut ligacoes = Vec::new();
    for (de, fks) in chaves.iter().enumerate() {
        for (_, def) in fks {
            let Some(nome) = referencia(def) else { continue };
            if let Some(para) = tabelas.iter().position(|t| t.nome == nome) {
                ligacoes.push((de, para));
            }
        }
    }
    Ok(Diagrama { caixas, ligacoes, de_fora })
}

fn altura(c: &Caixa) -> f32 {
    CABECA + LINHA * (c.colunas.len().min(MAX_COLUNAS) + usize::from(c.colunas.len() > MAX_COLUNAS)) as f32 + 6.0
}

/// Onde cada caixa fica (relativo ao canto do desenho) e a altura total: três por fileira.
fn posicoes(d: &Diagrama) -> (Vec<Rect>, f32) {
    let mut r = Vec::new();
    let mut y = 0.0;
    for fileira in d.caixas.chunks(POR_LINHA) {
        let alta = fileira.iter().map(altura).fold(0.0, f32::max);
        for (i, c) in fileira.iter().enumerate() {
            r.push(Rect { x: VAO / 2.0 + i as f32 * (LARGURA + VAO), y: y + VAO / 2.0, width: LARGURA, height: altura(c) });
        }
        y += alta + VAO;
    }
    (r, y.max(VAO))
}

impl App {
    pub(crate) fn view_diagrama<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some((alvo, estado)) = &self.diagrama else { return base };
        let fechar = Msg::Diagrama(None);
        let corpo: Element<'a, Msg> = match estado {
            None => pelicano::text(tr("Lendo as tabelas e as chaves…")).size(13).color(COR_TEXTO_MUTED()).into(),
            Some(Err(e)) => pelicano::text(e.clone()).size(13).color(COR_ERRO()).into(),
            Some(Ok(d)) if d.caixas.is_empty() => pelicano::text(tr("Nenhuma tabela neste banco.")).size(13).color(COR_TEXTO_MUTED()).into(),
            Some(Ok(d)) => desenho(d),
        };
        let nome = alvo.1.clone().unwrap_or_else(|| self.itens.iter().find(|i| i.id == alvo.0).map(|i| i.conexao.nome.clone()).unwrap_or_default());
        let mut dialogo = feather_ui::dialogo::Dialogo::new(trf("Diagrama de {}", &[&nome]), corpo)
            .icone("sitemap")
            .largura(POR_LINHA as f32 * (LARGURA + VAO) + 40.0)
            .altura_maxima(640.0)
            .ao_fechar(fechar.clone())
            .primario(tr("Fechar"), Some(fechar));
        if let Some(Ok(d)) = estado {
            let mut nota = trf("{} tabelas, {} ligações", &[&d.caixas.len(), &d.ligacoes.len()]);
            if d.de_fora > 0 {
                nota = format!("{nota} · {}", trf("{} tabelas ficaram de fora", &[&d.de_fora]));
            }
            dialogo = dialogo.subtitulo(nota);
        }
        dialogo.sobre(base)
    }

    pub(crate) fn abrir_diagrama(&mut self, alvo: Alvo) -> pelicano::Task<Msg> {
        let Some(b) = self.base(&alvo) else { return pelicano::Task::none() };
        let Some(banco) = b.banco.clone() else { return pelicano::Task::none() };
        let tabelas = b.tabelas.clone();
        self.diagrama = Some((alvo.clone(), None));
        crate::em_segundo_plano(move || ler(&banco, tabelas), move |r| Msg::Diagrama(Some((alvo, Some(r)))))
    }
}

fn desenho(d: &Diagrama) -> Element<'_, Msg> {
    let (pos, alto) = posicoes(d);
    pelicano::canvas(POR_LINHA as f32 * (LARGURA + VAO), alto, move |r, a, _| {
        let em = |p: &Rect| Rect { x: a.x + p.x, y: a.y + p.y, ..*p };
        // As linhas primeiro, por baixo das caixas. Andam só pelos vãos (entre colunas e acima da fileira
        // do alvo), para nunca passar por cima de outra tabela; cada uma com um desvio para não se sobreporem.
        let cor = COR_AZUL_ACENTO().alpha(0.7);
        for (k, &(de, para)) in d.ligacoes.iter().enumerate() {
            let (o, p) = (em(&pos[de]), em(&pos[para]));
            let (sy, ty) = (o.y + CABECA / 2.0, p.y + CABECA / 2.0);
            if de == para {
                let x = o.x + o.width;
                r.stroke(&[Point::new(x, sy), Point::new(x + 14.0, sy), Point::new(x + 14.0, sy + 14.0), Point::new(x, sy + 14.0)], 1.5, cor);
                continue;
            }
            let desvio = (k % 5) as f32 * 4.0 - 8.0;
            let direita = p.x >= o.x;
            let (sx, gx1) = if direita { (o.x + o.width, o.x + o.width + VAO / 2.0) } else { (o.x, o.x - VAO / 2.0) };
            let (tx, gx2) = if p.x > o.x { (p.x, p.x - VAO / 2.0) } else { (p.x + p.width, p.x + p.width + VAO / 2.0) };
            let (gx1, gx2, ry) = (gx1 + desvio, gx2 + desvio, p.y - VAO / 2.0 + desvio);
            let pontos = if o.y == p.y && (gx1 - gx2).abs() < 1.0 {
                vec![Point::new(sx, sy), Point::new(gx1, sy), Point::new(gx1, ty), Point::new(tx, ty)]
            } else {
                vec![Point::new(sx, sy), Point::new(gx1, sy), Point::new(gx1, ry), Point::new(gx2, ry), Point::new(gx2, ty), Point::new(tx, ty)]
            };
            r.stroke(&pontos, 1.5, cor);
            r.fill_quad(Rect { x: tx - 3.0, y: ty - 3.0, width: 6.0, height: 6.0 }, Some(&cor.into()), Border::rounded(3.0), None);
        }
        for (c, p) in d.caixas.iter().zip(&pos) {
            let p = em(p);
            r.fill_quad(p, Some(&COR_PAINEL().into()), Border::rounded(6.0).color(COR_BORDA(), 1.0), None);
            r.fill_quad(Rect { height: CABECA, ..p }, Some(&COR_PAINEL_FAIXA().into()), Border { radius: [6.0, 6.0, 0.0, 0.0], ..Border::default() }, None);
            r.draw_text(&c.nome, 12.5, Font::SEMIBOLD, COR_TEXTO(), Point::new(p.x + 10.0, p.y + 5.0), Some(p.width - 20.0));
            for (i, col) in c.colunas.iter().take(MAX_COLUNAS).enumerate() {
                let y = p.y + CABECA + 3.0 + i as f32 * LINHA;
                let cor = if col.chave { COR_AZUL_ACENTO() } else { COR_TEXTO() };
                r.draw_text(&col.nome, 11.5, if col.chave { Font::SEMIBOLD } else { Font::DEFAULT }, cor, Point::new(p.x + 10.0, y), Some(p.width * 0.55));
                r.draw_text(&col.tipo, 10.5, Font::MONO, COR_TEXTO_MUTED(), Point::new(p.x + p.width * 0.6, y + 1.0), Some(p.width * 0.4 - 10.0));
            }
            if c.colunas.len() > MAX_COLUNAS {
                let y = p.y + CABECA + 3.0 + MAX_COLUNAS as f32 * LINHA;
                r.draw_text(&trf("+{} colunas", &[&(c.colunas.len() - MAX_COLUNAS)]), 11.0, Font::DEFAULT, COR_TEXTO_MUTED(), Point::new(p.x + 10.0, y), None);
            }
        }
    })
    .into()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn tabela_da_chave_estrangeira() {
        assert_eq!(referencia("FOREIGN KEY (pessoa) REFERENCES pessoas(id)").as_deref(), Some("pessoas"));
        assert_eq!(referencia("FOREIGN KEY (a) REFERENCES vendas.\"Cliente\"(id) ON DELETE CASCADE").as_deref(), Some("Cliente"));
        assert_eq!(referencia("FOREIGN KEY (a, b) REFERENCES `x` (a, b)").as_deref(), Some("x"));
        assert_eq!(referencia("CHECK (a > 0)"), None);
    }

    #[test]
    fn caixas_tres_por_fileira() {
        let c = |n| Caixa { nome: "t".into(), colunas: vec![Coluna::default(); n] };
        let d = Diagrama { caixas: vec![c(2), c(20), c(1), c(1)], ligacoes: vec![], de_fora: 0 };
        let (p, alto) = posicoes(&d);
        assert_eq!(p[1].height, CABECA + LINHA * 13.0 + 6.0);
        assert_eq!(p[3].y - p[0].y, p[1].height + VAO);
        assert_eq!(alto, p[3].y + p[3].height + VAO / 2.0);
    }
}

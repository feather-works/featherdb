//! A grade do resultado (em páginas, para a tela ficar leve com mil linhas), as cores do SQL
//! no editor e a cópia em texto separado (TSV para colar na planilha, CSV para exportar).
use feather_ui::tema::*;
use featherdb_core::Resultado;
use pelicano::{column, container, row, text, Align, Color, Element, Font, Length, Space, Style};

use crate::Msg;

const POR_PAGINA: usize = 200;
const LARGURA_MIN: f32 = 60.0;
const LARGURA_MAX: f32 = 320.0;
const ALTURA_LINHA: f32 = 30.0;

/// O que cabe numa célula: uma linha, no máximo 200 caracteres. Um JSON de 1 MB numa coluna
/// seria medido e desenhado inteiro a cada quadro; a cópia e o CSV continuam com o valor todo.
fn resumo(v: &str) -> String {
    v.chars().take(200).map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect()
}

pub(crate) fn paginas(linhas: usize) -> usize {
    linhas.div_ceil(POR_PAGINA).max(1)
}

/// Largura de cada coluna pelo texto mais comprido (cabeçalho e linhas da página).
fn larguras(r: &Resultado, linhas: &[Vec<Option<String>>]) -> Vec<f32> {
    r.colunas
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let maior = linhas.iter().map(|l| l.get(i).and_then(|v| v.as_ref()).map_or(4, |v| v.chars().take(60).count())).max().unwrap_or(0).max(c.chars().count());
            (maior as f32 * 7.4 + 20.0).clamp(LARGURA_MIN, LARGURA_MAX)
        })
        .collect()
}

pub(crate) fn view(r: &Resultado, pagina: usize) -> Element<'_, Msg> {
    let inicio = (pagina * POR_PAGINA).min(r.linhas.len());
    let linhas = &r.linhas[inicio..(inicio + POR_PAGINA).min(r.linhas.len())];
    let larg = larguras(r, linhas);
    // Coluna só de números (os não nulos da página): alinhada à direita, como numa planilha.
    let numerica: Vec<bool> = (0..r.colunas.len())
        .map(|i| {
            let mut vals = linhas.iter().filter_map(|l| l.get(i).and_then(|v| v.as_deref())).peekable();
            vals.peek().is_some() && vals.all(|v| v.trim().parse::<f64>().is_ok())
        })
        .collect();
    let largura_total = 52.0 + larg.iter().sum::<f32>() + larg.len() as f32;
    let celula = |conteudo: Element<'static, Msg>, w: f32, direita: bool| {
        container(conteudo).width(w).height(ALTURA_LINHA).padding([0, 10]).align_y(Align::Center).align_x(if direita { Align::End } else { Align::Start })
    };
    // Linha fina entre as colunas.
    let separador = || container(Space::new(1, ALTURA_LINHA)).style(|_| Style::default().background(COR_BORDA_SUAVE()));
    let mut cab = row![celula(text("#").size(11.5).color(COR_TEXTO_MUTED()).into(), 52.0, true)];
    for ((c, w), num) in r.colunas.iter().zip(&larg).zip(&numerica) {
        cab = cab.push(separador());
        cab = cab.push(celula(text(c.clone()).size(12.5).font(Font::SEMIBOLD).no_wrap().ellipsis().into(), *w, *num));
    }
    let cab = container(cab).style(|_| Style::default().background(COR_PAINEL_FAIXA()));
    let linha_h = || container(Space::new(largura_total, 1)).style(|_| Style::default().background(COR_BORDA()));
    let mut corpo = column![cab, linha_h()];
    for (n, l) in linhas.iter().enumerate() {
        let mut linha = row![celula(text((inicio + n + 1).to_string()).size(11.5).color(COR_TEXTO_MUTED()).into(), 52.0, true)];
        for ((v, w), num) in l.iter().zip(&larg).zip(&numerica) {
            let t: Element<'static, Msg> = match v {
                Some(v) => text(resumo(v)).size(12.5).no_wrap().ellipsis().into(),
                None => text("NULL").size(11.5).font(Font::MONO).color(COR_TEXTO_MUTED()).into(),
            };
            linha = linha.push(separador());
            linha = linha.push(celula(t, *w, *num));
        }
        let par = n % 2 == 1;
        corpo = corpo.push(container(linha).style(move |_| if par { Style::default().background(COR_PAINEL_FAIXA()) } else { Style::default() }));
    }
    rolagem(corpo).direction(pelicano::Direction::Both).width(Length::FILL).height(Length::FILL).into()
}

const PALAVRAS: &[&str] = &[
    "ADD", "ALL", "ALTER", "AND", "AS", "ASC", "BEGIN", "BETWEEN", "BY", "CASE", "CAST", "COMMIT", "CREATE", "CROSS", "DEFAULT", "DELETE", "DESC",
    "DISTINCT", "DROP", "ELSE", "END", "EXISTS", "FALSE", "FROM", "FULL", "GROUP", "HAVING", "IN", "INDEX", "INNER", "INSERT", "INTO", "IS", "JOIN",
    "KEY", "LEFT", "LIKE", "LIMIT", "NOT", "NULL", "OFFSET", "ON", "OR", "ORDER", "OUTER", "OVER", "PARTITION", "PRIMARY", "REFERENCES", "RIGHT",
    "ROLLBACK", "SELECT", "SET", "TABLE", "THEN", "TOP", "TRUE", "TRUNCATE", "UNION", "UPDATE", "USING", "VALUES", "VIEW", "WHEN", "WHERE", "WITH",
];

/// Cores de uma linha do editor: palavras-chave na cor de destaque; textos e comentários apagados.
pub(crate) fn cores_sql(linha: &str, chave: Color, apagado: Color) -> Vec<(std::ops::Range<usize>, Color)> {
    let b = linha.as_bytes();
    let mut saida = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'-' && b.get(i + 1) == Some(&b'-') {
            saida.push((i..b.len(), apagado));
            break;
        }
        if c == b'\'' {
            let fim = linha[i + 1..].find('\'').map_or(b.len(), |f| i + f + 2);
            saida.push((i..fim, apagado));
            i = fim;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let fim = linha[i..].find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).map_or(b.len(), |f| i + f);
            if PALAVRAS.binary_search(&linha[i..fim].to_ascii_uppercase().as_str()).is_ok() {
                saida.push((i..fim, chave));
            }
            i = fim;
            continue;
        }
        i += 1;
    }
    saida
}

/// O resultado como texto separado (cabeçalho + linhas). Campo com separador, aspas ou quebra
/// de linha vai entre aspas, como o Excel e o FeatherSheet esperam.
pub(crate) fn separado(r: &Resultado, sep: char) -> String {
    let campo = |s: &str| {
        if s.contains([sep, '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_owned() }
    };
    let mut saida = r.colunas.iter().map(|c| campo(c)).collect::<Vec<_>>().join(&sep.to_string());
    for l in &r.linhas {
        saida.push_str("\r\n");
        saida.push_str(&l.iter().map(|v| v.as_deref().map_or(String::new(), campo)).collect::<Vec<_>>().join(&sep.to_string()));
    }
    saida.push_str("\r\n");
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn palavras_em_ordem_para_a_busca() {
        assert!(PALAVRAS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn cores_da_linha() {
        let (k, a) = (Color::BLACK, Color::WHITE);
        let c = cores_sql("select nome from t where x = 'from' -- fim", k, a);
        let trechos: Vec<_> = c.iter().map(|(r, _)| r.clone()).collect();
        assert_eq!(trechos, vec![0..6, 12..16, 19..24, 29..35, 36..42]);
    }

    #[test]
    fn csv_com_aspas_e_nulo() {
        let r = Resultado { colunas: vec!["a".into(), "b".into()], linhas: vec![vec![Some("x,y".into()), None], vec![Some("diz \"oi\"".into()), Some("2".into())]], ..Default::default() };
        assert_eq!(separado(&r, ','), "a,b\r\n\"x,y\",\r\n\"diz \"\"oi\"\"\",2\r\n");
    }
}

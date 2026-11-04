//! A grade do resultado (em páginas, para a tela ficar leve com mil linhas), as cores do SQL
//! no editor e a cópia em texto separado (TSV para colar na planilha, CSV para exportar).
use feather_ui::tema::*;
use featherdb_core::Resultado;
use pelicano::{column, container, row, text, Color, Element, Font, Length, Style};

use crate::Msg;

const POR_PAGINA: usize = 200;
const LARGURA_MIN: f32 = 60.0;
const LARGURA_MAX: f32 = 320.0;

pub(crate) fn paginas(linhas: usize) -> usize {
    linhas.div_ceil(POR_PAGINA).max(1)
}

/// Largura de cada coluna pelo texto mais comprido (cabeçalho e linhas da página).
fn larguras(r: &Resultado, linhas: &[Vec<Option<String>>]) -> Vec<f32> {
    r.colunas
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let maior = linhas.iter().map(|l| l.get(i).and_then(|v| v.as_ref()).map_or(4, |v| v.chars().count())).max().unwrap_or(0).max(c.chars().count());
            (maior as f32 * 7.4 + 20.0).clamp(LARGURA_MIN, LARGURA_MAX)
        })
        .collect()
}

pub(crate) fn view(r: &Resultado, pagina: usize) -> Element<'_, Msg> {
    let inicio = (pagina * POR_PAGINA).min(r.linhas.len());
    let linhas = &r.linhas[inicio..(inicio + POR_PAGINA).min(r.linhas.len())];
    let larg = larguras(r, linhas);
    let celula = |conteudo: Element<'static, Msg>, w: f32| container(conteudo).width(w).padding([4, 8]);
    let mut cab = row![celula(text("#").size(12).color(COR_TEXTO_MUTED()).into(), 52.0)];
    for (c, w) in r.colunas.iter().zip(&larg) {
        cab = cab.push(celula(text(c.clone()).size(12.5).font(Font::SEMIBOLD).no_wrap().ellipsis().into(), *w));
    }
    let cab = container(cab).style(|_| Style::default().background(COR_PAINEL_FAIXA()));
    let mut corpo = column![cab];
    for (n, l) in linhas.iter().enumerate() {
        let mut linha = row![celula(text((inicio + n + 1).to_string()).size(12).color(COR_TEXTO_MUTED()).into(), 52.0)];
        for (v, w) in l.iter().zip(&larg) {
            let t: Element<'static, Msg> = match v {
                Some(v) => text(v.replace(['\n', '\r'], " ")).size(12.5).no_wrap().ellipsis().into(),
                None => text("NULL").size(12).color(COR_TEXTO_MUTED()).into(),
            };
            linha = linha.push(celula(t, *w));
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

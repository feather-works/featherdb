//! A grade do resultado (em páginas, para a tela ficar leve com mil linhas), as cores do SQL
//! no editor e a cópia em texto separado (TSV para colar na planilha, CSV para exportar).
use feather_ui::tema::*;
use featherdb_core::Resultado;
use featherdb_core::estrutura::{familia, Familia};
use pelicano::{column, container, row, text, Align, Color, Element, Font, Length, Space, Style};

use crate::Msg;

const LARGURA_MIN: f32 = 70.0;
const LARGURA_MAX: f32 = 320.0;
const LARGURA_NUM: f32 = 52.0;
pub(crate) const ALTURA_LINHA: f32 = 30.0;
/// O cabeçalho tem duas linhas: o nome da coluna e o tipo embaixo.
const ALTURA_CABECALHO: f32 = 46.0;

/// O que cabe numa célula: uma linha, no máximo 200 caracteres. Um JSON de 1 MB numa coluna
/// seria medido e desenhado inteiro a cada quadro; a cópia e o CSV continuam com o valor todo.
fn resumo(v: &str) -> String {
    v.chars().take(200).map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect()
}

/// Largura de cada coluna pelo texto mais comprido (cabeçalho, tipo e as primeiras 200 linhas).
fn larguras(r: &Resultado) -> Vec<f32> {
    let amostra = &r.linhas[..r.linhas.len().min(200)];
    r.colunas
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let tipo = r.tipos.get(i).map_or(0, |t| t.chars().count());
            let maior = amostra.iter().map(|l| l.get(i).and_then(|v| v.as_ref()).map_or(4, |v| v.chars().take(60).count())).max().unwrap_or(0).max(c.chars().count() + 3).max(tipo);
            (maior as f32 * 7.4 + 22.0).clamp(LARGURA_MIN, LARGURA_MAX)
        })
        .collect()
}

// Ícones de traço do Tabler para a família do tipo.
const NUMERO: &str = "M3 10l2 -2v8M9 8h3a1 1 0 0 1 1 1v2a1 1 0 0 1 -1 1h-2a1 1 0 0 0 -1 1v2a1 1 0 0 0 1 1h3M17 8h2.5a1.5 1.5 0 0 1 1.5 1.5v1a1.5 1.5 0 0 1 -1.5 1.5h-1.5h1.5a1.5 1.5 0 0 1 1.5 1.5v1a1.5 1.5 0 0 1 -1.5 1.5h-2.5";
const TEXTO: &str = "M17.5 15.5m-3.5 0a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0M3 19v-10.5a3.5 3.5 0 0 1 7 0v10.5M3 13h7M21 12v7";
const DATA: &str = "M4 7a2 2 0 0 1 2 -2h12a2 2 0 0 1 2 2v12a2 2 0 0 1 -2 2h-12a2 2 0 0 1 -2 -2v-12zM16 3v4M8 3v4M4 11h16";
const LOGICO: &str = "M8 12m-2 0a2 2 0 1 0 4 0a2 2 0 1 0 -4 0M2 12a6 6 0 0 1 6 -6h8a6 6 0 0 1 6 6a6 6 0 0 1 -6 6h-8a6 6 0 0 1 -6 -6z";
const JSON: &str = "M7 4a2 2 0 0 0 -2 2v3a2 3 0 0 1 -2 3a2 3 0 0 1 2 3v3a2 2 0 0 0 2 2M17 4a2 2 0 0 1 2 2v3a2 3 0 0 0 2 3a2 3 0 0 0 -2 3v3a2 2 0 0 1 -2 2";
const BINARIO: &str = "M14 3v4a1 1 0 0 0 1 1h4M17 21h-10a2 2 0 0 1 -2 -2v-14a2 2 0 0 1 2 -2h7l5 5v11a2 2 0 0 1 -2 2z";
const OUTRO: &str = "M12 12m-1 0a1 1 0 1 0 2 0a1 1 0 1 0 -2 0M12 12m-8 0a8 8 0 1 0 16 0a8 8 0 1 0 -16 0";
const SOBE: &str = "M12 5v14M18 11l-6 -6M6 11l6 -6";
const DESCE: &str = "M12 5v14M18 13l-6 6M6 13l6 6";
const CHAVE: &str = "M16.555 3.843l3.602 3.602a2.877 2.877 0 0 1 0 4.069l-2.643 2.643a2.877 2.877 0 0 1 -4.069 0l-.301 -.301l-6.558 6.558a2 2 0 0 1 -1.239 .578l-.175 .008h-1.172a1 1 0 0 1 -.993 -.883l-.007 -.117v-1.172a2 2 0 0 1 .467 -1.284l.119 -.13l.414 -.414h2v-2h2v-2l2.144 -2.144l-.301 -.301a2.877 2.877 0 0 1 0 -4.069l2.643 -2.643a2.877 2.877 0 0 1 4.069 0zM15 9h.01";

/// O ícone da família do tipo (números, texto, data, sim/não, JSON); a chave primária ganha a chave na cor de destaque.
pub(crate) fn icone_tipo(tipo: &str, chave: bool, tamanho: f32) -> pelicano::SvgView {
    if chave {
        return crate::icone_tabler(CHAVE, tamanho).color(COR_AZUL_ACENTO());
    }
    let caminho = match familia(tipo) {
        Familia::Numero => NUMERO,
        Familia::Texto => TEXTO,
        Familia::Data => DATA,
        Familia::Logico => LOGICO,
        Familia::Json => JSON,
        Familia::Binario => BINARIO,
        Familia::Outro => OUTRO,
    };
    crate::icone_tabler(caminho, tamanho).color(COR_TEXTO_MUTED())
}

/// As linhas na ordem da tela: só as que têm `filtro` em alguma célula, ordenadas pela coluna
/// escolhida (números como números, texto sem diferença de maiúscula; NULL primeiro).
pub(crate) fn ordem(r: &Resultado, ordenacao: Option<(usize, bool)>, filtro: &str) -> Vec<usize> {
    let filtro = filtro.trim().to_lowercase();
    let mut v: Vec<usize> = (0..r.linhas.len())
        .filter(|&i| filtro.is_empty() || r.linhas[i].iter().flatten().any(|c| c.to_lowercase().contains(&filtro)))
        .collect();
    if let Some((c, crescente)) = ordenacao {
        let valor = |i: usize| r.linhas[i].get(c).cloned().flatten();
        v.sort_by(|&a, &b| {
            let o = match (valor(a), valor(b)) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, _) => std::cmp::Ordering::Less,
                (_, None) => std::cmp::Ordering::Greater,
                (Some(x), Some(y)) => match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
                    (Ok(x), Ok(y)) => x.total_cmp(&y),
                    _ => x.to_lowercase().cmp(&y.to_lowercase()),
                },
            };
            if crescente { o } else { o.reverse() }
        });
    }
    v
}

/// A grade do resultado. Monta só as linhas e colunas que aparecem na tela (`pos` e `vista` vêm da
/// própria rolagem): mil linhas rolam tão leve quanto dez. O cabeçalho desce junto e fica sempre no topo.
/// `linhas` é a ordem da tela ([`ordem`]); clicar no nome da coluna ordena por ela.
pub(crate) fn view<'a>(r: &'a Resultado, linhas: &[usize], ordenacao: Option<(usize, bool)>, pos: (f32, f32), vista: (f32, f32), versao: u64) -> Element<'a, Msg> {
    let larg = larguras(r);
    // Início de cada coluna (com a linha de 1 px entre elas), depois da coluna do número.
    let mut pre = vec![0.0f32];
    for w in &larg {
        pre.push(pre.last().unwrap() + w + 1.0);
    }
    let total_w = LARGURA_NUM + pre.last().copied().unwrap_or(0.0);
    let (x, y) = pos;
    let (vw, vh) = (vista.0.max(400.0), vista.1.max(200.0));
    let n = linhas.len();
    let ini = ((y - ALTURA_CABECALHO).max(0.0) / ALTURA_LINHA) as usize;
    let fim = (ini + (vh / ALTURA_LINHA) as usize + 2).min(n);
    let ini = ini.min(fim);
    let x_dados = (x - LARGURA_NUM).max(0.0);
    let c0 = pre.iter().skip(1).position(|f| *f > x_dados).unwrap_or(larg.len());
    let c1 = pre.iter().position(|p| *p >= x_dados + vw).unwrap_or(larg.len()).max(c0);
    let (esq, dir) = (pre[c0], pre[larg.len()] - pre[c1]);
    // Coluna de números: alinhada à direita (pelo tipo, ou pelos valores quando o banco não diz o tipo).
    let numerica: Vec<bool> = (0..r.colunas.len())
        .map(|i| match r.tipos.get(i).filter(|t| !t.is_empty()) {
            Some(t) => familia(t) == Familia::Numero,
            None => {
                let mut vals = r.linhas.iter().take(200).filter_map(|l| l.get(i).and_then(|v| v.as_deref())).peekable();
                vals.peek().is_some() && vals.all(|v| v.trim().parse::<f64>().is_ok())
            }
        })
        .collect();
    let celula = |conteudo: Element<'static, Msg>, w: f32, h: f32, direita: bool| -> Element<'static, Msg> {
        container(conteudo).width(w).height(h).padding([0, 10]).align_y(Align::Center).align_x(if direita { Align::End } else { Align::Start }).into()
    };
    let separador = |h: f32| -> Element<'static, Msg> { container(Space::new(1, h)).style(|_| Style::default().background(COR_BORDA_SUAVE())).into() };
    // Uma faixa: o número, o espaço das colunas à esquerda, as colunas à mostra e o espaço das da direita.
    let faixa = |num: Element<'static, Msg>, h: f32, cel: &dyn Fn(usize) -> Element<'static, Msg>| -> Element<'static, Msg> {
        let mut r = row![num, Space::new(esq, h)];
        for c in c0..c1 {
            r = r.push(separador(h));
            r = r.push(cel(c));
        }
        r.push(Space::new(dir, h)).into()
    };

    let faixas = (ini..fim).map(|i| {
        let l = &r.linhas[linhas[i]];
        let num = celula(text((i + 1).to_string()).size(11.5).color(COR_TEXTO_MUTED()).into(), LARGURA_NUM, ALTURA_LINHA, true);
        let cel = |c: usize| {
            let t: Element<'static, Msg> = match l.get(c).and_then(|v| v.as_deref()) {
                Some(v) => text(resumo(v)).size(12.5).no_wrap().ellipsis().into(),
                None => text("NULL").size(11.5).font(Font::MONO).color(COR_TEXTO_MUTED()).into(),
            };
            celula(t, larg[c], ALTURA_LINHA, numerica[c])
        };
        let par = i % 2 == 1;
        container(faixa(num, ALTURA_LINHA, &cel))
            .width(total_w)
            .height(ALTURA_LINHA)
            .style(move |_| if par { Style::default().background(COR_PAINEL_FAIXA()) } else { Style::default() })
            .into()
    });
    let corpo = column(
        std::iter::once(Space::new(total_w, ALTURA_CABECALHO + 1.0 + ini as f32 * ALTURA_LINHA).into())
            .chain(faixas)
            .chain([Space::new(total_w, (n - fim) as f32 * ALTURA_LINHA).into()])
            .collect::<Vec<Element<'static, Msg>>>(),
    );

    let num = celula(text("#").size(11.5).color(COR_TEXTO_MUTED()).into(), LARGURA_NUM, ALTURA_CABECALHO, true);
    let cab_cel = |c: usize| {
        let tipo = r.tipos.get(c).cloned().unwrap_or_default();
        let mut nome = row![].spacing(6).align(Align::Center);
        if !tipo.is_empty() {
            nome = nome.push(icone_tipo(&tipo, false, 14.0));
        }
        let mut nome = nome.push(text(r.colunas[c].clone()).size(12.5).font(Font::SEMIBOLD).no_wrap().ellipsis());
        if let Some((_, crescente)) = ordenacao.filter(|(o, _)| *o == c) {
            nome = nome.push(crate::icone_tabler(if crescente { SOBE } else { DESCE }, 13.0).color(COR_AZUL_ACENTO()));
        }
        let conteudo: Element<'static, Msg> = column![nome, text(tipo).size(11).font(Font::MONO).color(COR_TEXTO_MUTED()).no_wrap().ellipsis()].spacing(2).into();
        let botao = pelicano::button(celula(conteudo, larg[c], ALTURA_CABECALHO, false)).padding(0).style(estilo_botao_pequeno(false)).on_press(Msg::Ordenar(c));
        pelicano::tooltip(botao, crate::idioma::tr("Clique para ordenar"), pelicano::TooltipPosition::Bottom).padding([4, 8]).style(estilo_dica).into()
    };
    let cab = column![
        container(faixa(num, ALTURA_CABECALHO, &cab_cel)).width(total_w).height(ALTURA_CABECALHO).style(|_| Style::default().background(COR_PAINEL_FAIXA())),
        container(Space::new(total_w, 1)).style(|_| Style::default().background(COR_BORDA())),
    ];
    // O cabeçalho fica por cima, descido o tanto que a rolagem desceu: sempre visível, e rola de lado com as células.
    let fixo = column![Space::new(total_w, y), pelicano::opaque(cab)];
    rolagem(pelicano::stack![corpo, fixo])
        .direction(pelicano::Direction::Both)
        .id(("grade", versao))
        .on_scroll(|v| Msg::Rolou(v.offset.x, v.offset.y, v.bounds.width, v.bounds.height))
        .width(Length::FILL)
        .height(Length::FILL)
        .into()
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
pub(crate) fn separado(r: &Resultado, linhas: &[usize], sep: char) -> String {
    let campo = |s: &str| {
        if s.contains([sep, '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_owned() }
    };
    let mut saida = r.colunas.iter().map(|c| campo(c)).collect::<Vec<_>>().join(&sep.to_string());
    for l in linhas.iter().map(|&i| &r.linhas[i]) {
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
    fn ordena_numero_como_numero_e_filtra() {
        let l = |v: &[Option<&str>]| v.iter().map(|x| x.map(str::to_owned)).collect::<Vec<_>>();
        let r = Resultado { colunas: vec!["n".into(), "nome".into()], linhas: vec![l(&[Some("10"), Some("Ana")]), l(&[Some("9"), Some("bia")]), l(&[None, Some("Caio")])], ..Default::default() };
        assert_eq!(ordem(&r, Some((0, true)), ""), vec![2, 1, 0]);
        assert_eq!(ordem(&r, Some((0, false)), ""), vec![0, 1, 2]);
        assert_eq!(ordem(&r, Some((1, false)), ""), vec![2, 1, 0]);
        assert_eq!(ordem(&r, None, "BI"), vec![1]);
    }

    #[test]
    fn csv_com_aspas_e_nulo() {
        let r = Resultado { colunas: vec!["a".into(), "b".into()], linhas: vec![vec![Some("x,y".into()), None], vec![Some("diz \"oi\"".into()), Some("2".into())]], ..Default::default() };
        assert_eq!(separado(&r, &[0, 1], ','), "a,b\r\n\"x,y\",\r\n\"diz \"\"oi\"\"\",2\r\n");
    }
}

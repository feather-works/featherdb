//! Importar CSV para uma tabela: ler o arquivo (separador `,` `;` ou tabulação, aspas como o Excel)
//! e montar os INSERT em lotes.
use crate::{estrutura, identificador, Tipo};

/// O separador mais provável: o que mais aparece na primeira linha (fora de aspas não importa aqui).
fn separador(texto: &str) -> char {
    let linha = texto.lines().next().unwrap_or("");
    [',', ';', '\t', '|'].into_iter().max_by_key(|c| linha.matches(*c).count()).unwrap_or(',')
}

/// O cabeçalho e as linhas do CSV. Campo vazio vira `None` (NULL no banco).
pub fn ler_csv(texto: &str) -> (Vec<String>, Vec<Vec<Option<String>>>) {
    let texto = texto.trim_start_matches('\u{feff}');
    let sep = separador(texto);
    let mut linhas: Vec<Vec<String>> = Vec::new();
    let (mut linha, mut campo, mut aspas) = (Vec::new(), String::new(), false);
    let mut c = texto.chars().peekable();
    while let Some(ch) = c.next() {
        match ch {
            '"' if aspas && c.peek() == Some(&'"') => {
                campo.push('"');
                c.next();
            }
            '"' => aspas = !aspas,
            _ if aspas => campo.push(ch),
            _ if ch == sep => linha.push(std::mem::take(&mut campo)),
            '\r' => {}
            '\n' => {
                linha.push(std::mem::take(&mut campo));
                linhas.push(std::mem::take(&mut linha));
            }
            _ => campo.push(ch),
        }
    }
    if !campo.is_empty() || !linha.is_empty() {
        linha.push(campo);
        linhas.push(linha);
    }
    linhas.retain(|l| !(l.len() == 1 && l[0].trim().is_empty()));
    let mut it = linhas.into_iter();
    let cabecalho = it.next().unwrap_or_default().into_iter().map(|s| s.trim().to_owned()).collect();
    let dados = it.map(|l| l.into_iter().map(|v| (!v.is_empty()).then_some(v)).collect()).collect();
    (cabecalho, dados)
}

/// Os INSERT, `lote` linhas por comando. `mapa[i]` é a coluna da tabela que recebe a coluna `i` do CSV
/// (`None`: a coluna do CSV fica de fora).
pub fn sql_importar(tipo: Tipo, tabela: &str, mapa: &[Option<String>], linhas: &[Vec<Option<String>>], lote: usize) -> Vec<String> {
    let usadas: Vec<(usize, &String)> = mapa.iter().enumerate().filter_map(|(i, m)| m.as_ref().map(|n| (i, n))).collect();
    if usadas.is_empty() {
        return Vec::new();
    }
    let nomes: Vec<String> = usadas.iter().map(|(_, n)| identificador(n, tipo)).collect();
    linhas
        .chunks(lote.max(1))
        .map(|bloco| {
            let valores: Vec<String> = bloco
                .iter()
                .map(|l| format!("({})", usadas.iter().map(|(i, _)| estrutura::valor_sql(tipo, l.get(*i).and_then(|v| v.as_deref()))).collect::<Vec<_>>().join(", ")))
                .collect();
            format!("INSERT INTO {tabela} ({}) VALUES\n{}", nomes.join(", "), valores.join(",\n"))
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn csv_do_excel_com_ponto_e_virgula_e_aspas() {
        let (cab, l) = ler_csv("\u{feff}id;nome;obs\r\n1;\"Silva; Ana\";\r\n2;\"diz \"\"oi\"\"\nduas linhas\";x\r\n");
        assert_eq!(cab, ["id", "nome", "obs"]);
        assert_eq!(l[0], vec![Some("1".into()), Some("Silva; Ana".into()), None]);
        assert_eq!(l[1][1].as_deref(), Some("diz \"oi\"\nduas linhas"));
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn insert_em_lotes_so_com_as_colunas_mapeadas() {
        let linhas = vec![vec![Some("1".into()), Some("x".into())], vec![Some("2".into()), None], vec![Some("3".into()), Some("z".into())]];
        let s = sql_importar(Tipo::Postgres, "t", &[None, Some("Nome".into())], &linhas, 2);
        assert_eq!(s, vec!["INSERT INTO t (\"Nome\") VALUES\n('x'),\n(NULL)", "INSERT INTO t (\"Nome\") VALUES\n('z')"]);
    }
}

//! Deixa o SQL legível: palavras-chave em maiúscula, cada cláusula (SELECT, FROM, WHERE…) numa
//! linha, AND/OR e os itens do SELECT recuados. Textos, nomes entre aspas e comentários ficam como estão.

/// As palavras-chave do SQL, em ordem (busca binária; também colorem o editor).
pub const PALAVRAS: &[&str] = &[
    "ADD", "ALL", "ALTER", "AND", "AS", "ASC", "BEGIN", "BETWEEN", "BY", "CASE", "CAST", "COMMIT", "CREATE", "CROSS", "DEFAULT", "DELETE", "DESC",
    "DISTINCT", "DROP", "ELSE", "END", "EXISTS", "EXPLAIN", "FALSE", "FROM", "FULL", "GROUP", "HAVING", "IN", "INDEX", "INNER", "INSERT", "INTO", "IS",
    "JOIN", "KEY", "LEFT", "LIKE", "LIMIT", "NOT", "NULL", "OFFSET", "ON", "OR", "ORDER", "OUTER", "OVER", "PARTITION", "PRIMARY", "REFERENCES",
    "RETURNING", "RIGHT", "ROLLBACK", "SELECT", "SET", "TABLE", "THEN", "TOP", "TRUE", "TRUNCATE", "UNION", "UPDATE", "USING", "VALUES", "VIEW", "WHEN",
    "WHERE", "WITH",
];

pub fn palavra_chave(p: &str) -> bool {
    PALAVRAS.binary_search(&p.to_ascii_uppercase().as_str()).is_ok()
}

#[derive(Debug, PartialEq)]
enum Pedaco<'a> {
    Palavra(&'a str),
    /// Texto, nome entre aspas, número, símbolo: vai como está.
    Outro(&'a str),
    Comentario(&'a str),
}

/// Os pedaços do SQL e, para cada um, se ele vinha colado no anterior (sem espaço no meio).
fn pedacos(sql: &str) -> (Vec<Pedaco<'_>>, Vec<bool>) {
    let b = sql.as_bytes();
    let mut saida = Vec::new();
    let mut colado = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let ini = i;
        if !c.is_ascii_whitespace() && colado.len() == saida.len() {
            colado.push(ini > 0 && !b[ini - 1].is_ascii_whitespace());
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'-' && b.get(i + 1) == Some(&b'-') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            saida.push(Pedaco::Comentario(&sql[ini..i]));
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(b.len());
            saida.push(Pedaco::Comentario(&sql[ini..i]));
            continue;
        }
        if matches!(c, b'\'' | b'"' | b'`') {
            i += 1;
            // A aspa dobrada ('O''Neil') é parte do texto, não o fim.
            while i < b.len() && (b[i] != c || b.get(i + 1) == Some(&c)) {
                i += if b[i] == c { 2 } else { 1 };
            }
            i = (i + 1).min(b.len());
            saida.push(Pedaco::Outro(&sql[ini..i]));
            continue;
        }
        if c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80 || c == b'$' || c == b'.' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] >= 0x80 || b[i] == b'$' || b[i] == b'.') {
                i += 1;
            }
            saida.push(Pedaco::Palavra(&sql[ini..i]));
            continue;
        }
        // Operadores de dois caracteres juntos (<=, <>, ::, ||).
        i += 1;
        if i < b.len() && matches!((c, b[i]), (b'<', b'=' | b'>') | (b'>', b'=') | (b'!', b'=') | (b':', b':') | (b'|', b'|')) {
            i += 1;
        }
        saida.push(Pedaco::Outro(&sql[ini..i]));
    }
    (saida, colado)
}

/// Formata o SQL. Comandos que não são SQL (MongoDB) a interface não manda para cá.
pub fn formatar(sql: &str) -> String {
    // O que vinha colado continua colado: E'texto', [Nome Cliente], a='x'.
    let (p, colado) = pedacos(sql);
    let mut s = String::new();
    let mut nivel = 0usize;
    // Nos itens do SELECT (no nível de fora) cada vírgula quebra a linha.
    let mut na_lista: Vec<bool> = vec![false];
    let quebra = |s: &mut String, nivel: usize, extra: usize| {
        while s.ends_with(' ') {
            s.pop();
        }
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(&"  ".repeat(nivel + extra));
    };
    let mut anterior = String::new();
    let mut penultimo = String::new();
    // O AND de `BETWEEN x AND y` não quebra a linha.
    let mut entre = false;
    for (k, pedaco) in p.iter().enumerate() {
        let junto = colado[k];
        let proxima = match p.get(k + 1) {
            Some(Pedaco::Palavra(w)) => w.to_ascii_uppercase(),
            _ => String::new(),
        };
        match pedaco {
            Pedaco::Comentario(c) => {
                if !s.is_empty() && !s.ends_with('\n') && !s.ends_with(' ') {
                    s.push(' ');
                }
                s.push_str(c);
                if c.starts_with("--") {
                    quebra(&mut s, nivel, 0);
                } else {
                    s.push(' ');
                }
            }
            Pedaco::Palavra(w) => {
                let chave = palavra_chave(w);
                let m = w.to_ascii_uppercase();
                let texto = if chave { m.clone() } else { (*w).to_owned() };
                let junta_join = matches!(anterior.as_str(), "LEFT" | "RIGHT" | "INNER" | "FULL" | "CROSS" | "OUTER");
                let clausula = match m.as_str() {
                    "SELECT" | "FROM" | "WHERE" | "HAVING" | "LIMIT" | "OFFSET" | "UNION" | "VALUES" | "SET" | "RETURNING" | "WITH" | "UPDATE" | "DELETE" | "INSERT" => true,
                    "GROUP" | "ORDER" => proxima == "BY",
                    "LEFT" | "RIGHT" | "INNER" | "FULL" | "CROSS" => matches!(proxima.as_str(), "JOIN" | "OUTER"),
                    "JOIN" => !junta_join,
                    _ => false,
                } && !(m == "SELECT" && anterior == "(")
                    && !(anterior == "UNION" && m == "ALL")
                    && !(matches!(anterior.as_str(), "INSERT" | "DELETE") && m != "SELECT");
                if clausula {
                    quebra(&mut s, nivel, 0);
                    *na_lista.last_mut().unwrap() = m == "SELECT";
                } else if matches!(m.as_str(), "AND" | "OR") && !(entre && m == "AND") {
                    quebra(&mut s, nivel, 1);
                } else if !junto && !s.is_empty() && !s.ends_with(['\n', ' ', '(', '.', ':']) {
                    s.push(' ');
                }
                if m == "SELECT" && anterior == "(" {
                    *na_lista.last_mut().unwrap() = true;
                }
                if m == "BETWEEN" {
                    entre = true;
                } else if m == "AND" {
                    entre = false;
                }
                s.push_str(&texto);
                penultimo = std::mem::replace(&mut anterior, m);
            }
            Pedaco::Outro(o) => {
                match *o {
                    "," => {
                        while s.ends_with(' ') {
                            s.pop();
                        }
                        s.push(',');
                        if *na_lista.last().unwrap() {
                            quebra(&mut s, nivel, 1);
                        } else {
                            s.push(' ');
                        }
                    }
                    ";" => {
                        while s.ends_with([' ', '\n']) {
                            s.pop();
                        }
                        s.push_str(";\n\n");
                        nivel = 0;
                        na_lista = vec![false];
                    }
                    "(" => {
                        if !s.is_empty() && !s.ends_with(['\n', ' ', '(']) && !matches!(anterior.as_str(), "") {
                            // Função: sem espaço (count(*)); palavra-chave: com espaço (IN (...)).
                            if palavra_chave(&anterior) || matches!(penultimo.as_str(), "INTO" | "TABLE") {
                                s.push(' ');
                            }
                        } else if !s.is_empty() && !s.ends_with(['\n', ' ', '(']) {
                            s.push(' ');
                        }
                        s.push('(');
                        nivel += 1;
                        na_lista.push(false);
                    }
                    ")" => {
                        nivel = nivel.saturating_sub(1);
                        if na_lista.len() > 1 {
                            na_lista.pop();
                        }
                        while s.ends_with(' ') {
                            s.pop();
                        }
                        if s.ends_with('\n') || s.trim_end_matches(' ').ends_with('\n') {
                            s.push_str(&"  ".repeat(nivel));
                        }
                        s.push(')');
                    }
                    "." | "::" => {
                        while s.ends_with(' ') {
                            s.pop();
                        }
                        s.push_str(o);
                    }
                    _ => {
                        if !junto && !s.is_empty() && !s.ends_with(['\n', ' ', '(', '.']) && !s.ends_with("::") {
                            s.push(' ');
                        }
                        s.push_str(o);
                    }
                }
                penultimo = std::mem::replace(&mut anterior, (*o).to_owned());
            }
        }
    }
    let s = s.trim_end().to_owned();
    if s.is_empty() { s } else { s + "\n" }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn palavras_em_ordem_para_a_busca() {
        assert!(PALAVRAS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn select_com_join_e_where() {
        let f = formatar("select a.id, b.nome, count(*) from a left join b on b.id = a.b_id where a.x = 'and' and b.y in (1, 2) group by 1, 2 order by 2 desc limit 10");
        assert_eq!(
            f,
            "SELECT a.id,\n  b.nome,\n  count(*)\nFROM a\nLEFT JOIN b ON b.id = a.b_id\nWHERE a.x = 'and'\n  AND b.y IN (1, 2)\nGROUP BY 1, 2\nORDER BY 2 DESC\nLIMIT 10\n"
        );
    }

    #[test]
    fn varios_comandos_comentario_e_between() {
        let f = formatar("-- oi\nupdate t set a = 1, b = 2 where c between 1 and 5; delete from t where id = 3");
        assert_eq!(f, "-- oi\nUPDATE t\nSET a = 1, b = 2\nWHERE c BETWEEN 1 AND 5;\n\nDELETE FROM t\nWHERE id = 3\n");
    }

    #[test]
    fn texto_quebrado_nao_derruba() {
        for t in ["((", ")))", "'sem fim", "/* sem fim", "select ção, \"Nome\" from \"Tabela\"", "", ";;", "$$ x; $$"] {
            formatar(t);
        }
        assert_eq!(formatar("select ção from t"), "SELECT ção\nFROM t\n");
    }

    #[test]
    fn textos_e_nomes_colados_ficam_como_estao() {
        assert_eq!(formatar("select * from t where nome = 'O''Neil'"), "SELECT *\nFROM t\nWHERE nome = 'O''Neil'\n");
        assert_eq!(formatar("select E'a\\nb', [Nome Cliente] from [dbo].[t]"), "SELECT E'a\\nb',\n  [Nome Cliente]\nFROM [dbo].[t]\n");
        assert_eq!(formatar("select a, b from t"), "SELECT a,\n  b\nFROM t\n");
    }

    #[test]
    fn insert_e_cast_do_postgres() {
        assert_eq!(formatar("insert into t (a, b) values (1, 'x')"), "INSERT INTO t (a, b)\nVALUES (1, 'x')\n");
        assert_eq!(formatar("select x::text from t"), "SELECT x::text\nFROM t\n");
    }
}

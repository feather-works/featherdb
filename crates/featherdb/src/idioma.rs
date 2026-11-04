//! Tradução da interface. O texto em português escrito no código é a chave: em pt-BR [`tr`]
//! devolve o próprio texto; em en-US e es-ES procura em `idiomas/interface.<lingua>.json`
//! (mesmo jeito do FeatherSheet). Um teste confere que todo texto passado ao `tr` no código tem tradução.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

const EN: &str = include_str!("../idiomas/interface.en-US.json");
const ES: &str = include_str!("../idiomas/interface.es-ES.json");

pub(crate) const LINGUAS: [(&str, &str); 3] = [("pt-BR", "Português"), ("en-US", "English"), ("es-ES", "Español")];

static LINGUA: AtomicU8 = AtomicU8::new(0);

/// Chamado na abertura e ao trocar o idioma nas Configurações.
pub(crate) fn definir(codigo: &str) {
    let i = LINGUAS.iter().position(|(c, _)| codigo.starts_with(&c[..2])).unwrap_or(0);
    LINGUA.store(i as u8, Ordering::Relaxed);
    // Os textos da própria feather-ui (Ajuda, Novidades, Configurações) usam o mesmo dicionário.
    feather_ui::idioma::definir_tradutor(|pt| dicionario().and_then(|d| d.get(pt)).map(String::as_str));
}

/// A posição do idioma atual em [`LINGUAS`].
pub(crate) fn indice() -> usize {
    LINGUA.load(Ordering::Relaxed) as usize
}

/// O idioma do sistema (`LANG`), para a primeira abertura.
pub(crate) fn do_sistema() -> &'static str {
    let lang = std::env::var("LANG").unwrap_or_default();
    LINGUAS.iter().find(|(c, _)| lang.starts_with(&c[..2])).map_or("pt-BR", |(c, _)| c)
}

fn dicionario() -> Option<&'static HashMap<String, String>> {
    static D: OnceLock<[HashMap<String, String>; 2]> = OnceLock::new();
    let d = D.get_or_init(|| [ler(EN), ler(ES)]);
    match LINGUA.load(Ordering::Relaxed) {
        1 => Some(&d[0]),
        2 => Some(&d[1]),
        _ => None,
    }
}

fn ler(json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).expect("arquivo de idioma inválido")
}

/// O texto no idioma da interface (sem tradução, fica o português).
pub(crate) fn tr(pt: &str) -> &str {
    dicionario().and_then(|d| d.get(pt)).map_or(pt, String::as_str)
}

/// Como [`tr`], trocando cada `{}` pelos valores, na ordem.
pub(crate) fn trf(modelo: &str, valores: &[&dyn std::fmt::Display]) -> String {
    let mut saida = String::new();
    let mut resto = tr(modelo);
    for v in valores {
        let Some(i) = resto.find("{}") else { break };
        saida.push_str(&resto[..i]);
        saida.push_str(&v.to_string());
        resto = &resto[i + 2..];
    }
    saida.push_str(resto);
    saida
}

/// Os idiomas em que o texto falta (para os testes de textos montados fora de um `tr` literal).
#[cfg(test)]
pub(crate) fn sem_traducao(pt: &str) -> Vec<&'static str> {
    static D: OnceLock<[HashMap<String, String>; 2]> = OnceLock::new();
    let d = D.get_or_init(|| [ler(EN), ler(ES)]);
    [("en-US", &d[0]), ("es-ES", &d[1])].into_iter().filter(|(_, d)| !d.contains_key(pt)).map(|(n, _)| n).collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Os literais passados a `tr` e `trf` num arquivo, com os escapes do Rust resolvidos.
    fn chaves(fonte: &str) -> Vec<String> {
        let mut saida = Vec::new();
        for marca in ["tr(\"", "trf(\""] {
            let mut resto = fonte;
            while let Some(i) = resto.find(marca) {
                let antes = &resto[..i];
                resto = &resto[i + marca.len()..];
                // `fn tr(` e afins não contam; só a chamada.
                if antes.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let (mut s, mut chars) = (String::new(), resto.chars());
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some(o) => s.push(o),
                            None => {}
                        },
                        c => s.push(c),
                    }
                }
                saida.push(s);
            }
        }
        saida
    }

    #[test]
    fn todo_texto_da_interface_tem_traducao() {
        let (en, es) = (ler(EN), ler(ES));
        let pasta = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut faltam = Vec::new();
        for arq in std::fs::read_dir(pasta).unwrap() {
            let caminho = arq.unwrap().path();
            for chave in chaves(&std::fs::read_to_string(&caminho).unwrap()) {
                for (nome, d) in [("en-US", &en), ("es-ES", &es)] {
                    if !d.contains_key(&chave) {
                        faltam.push(format!("{nome}: {chave:?}"));
                    }
                }
            }
        }
        faltam.sort();
        faltam.dedup();
        assert!(faltam.is_empty(), "sem tradução:\n{}", faltam.join("\n"));
        let mut a: Vec<_> = en.keys().collect();
        let mut b: Vec<_> = es.keys().collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "en-US e es-ES precisam das mesmas chaves");
    }

    #[test]
    fn modelo_preenche_na_ordem() {
        assert_eq!(trf("{} de {}", &[&1, &2]), "1 de 2");
    }
}

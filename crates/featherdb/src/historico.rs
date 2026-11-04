//! Histórico das consultas rodadas: `historico.txt` na pasta de configuração, uma por linha
//! (`segundos \t onde \t sql`, com quebra de linha e tabulação escapadas). Só o SQL, nunca senha.
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use feather_ui::tema::*;
use pelicano::{button, column, text, text_input, Element, Font, Length};

use crate::idioma::{tr, trf};
use crate::{App, Msg};

/// Quantas ficam guardadas (o arquivo é cortado quando passa do dobro).
const GUARDAR: usize = 500;
const MOSTRAR: usize = 200;

pub(crate) struct Entrada {
    pub(crate) quando: u64,
    pub(crate) onde: String,
    pub(crate) sql: String,
}

fn arquivo() -> Option<std::path::PathBuf> {
    crate::config::pasta().map(|p| p.join("historico.txt"))
}

fn escapar(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n").replace('\t', "\\t").replace('\r', "")
}

fn desescapar(s: &str) -> String {
    let mut saida = String::with_capacity(s.len());
    let mut c = s.chars();
    while let Some(ch) = c.next() {
        if ch != '\\' {
            saida.push(ch);
            continue;
        }
        match c.next() {
            Some('n') => saida.push('\n'),
            Some('t') => saida.push('\t'),
            Some(o) => saida.push(o),
            None => saida.push('\\'),
        }
    }
    saida
}

fn linha(quando: u64, onde: &str, sql: &str) -> String {
    format!("{quando}\t{}\t{}\n", escapar(onde), escapar(sql))
}

fn de_texto(texto: &str) -> Vec<Entrada> {
    texto
        .lines()
        .filter_map(|l| {
            let mut p = l.splitn(3, '\t');
            Some(Entrada { quando: p.next()?.parse().ok()?, onde: desescapar(p.next()?), sql: desescapar(p.next()?) })
        })
        .collect()
}

/// Comando com senha (ALTER USER … PASSWORD, CREATE USER … IDENTIFIED BY): não vai para o arquivo.
fn tem_senha(sql: &str) -> bool {
    let l = sql.to_lowercase();
    l.contains("password") || l.contains("identified by")
}

fn agora() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Anota os comandos no fim do arquivo. Erro de disco não atrapalha a consulta.
pub(crate) fn anotar(comandos: &[String], onde: &str) {
    let Some(a) = arquivo() else { return };
    if let Some(p) = a.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let quando = agora();
    // Comando gigante (lote de importação) não entra: o histórico é para reaproveitar consultas.
    let novo: String = comandos.iter().filter(|c| !tem_senha(c) && c.len() < 20_000).map(|c| linha(quando, onde, c)).collect();
    if novo.is_empty() {
        return;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&a) {
        let _ = f.write_all(novo.as_bytes());
    }
    // Passou do dobro: fica só o fim (troca de uma vez, como o rascunho).
    if let Ok(texto) = std::fs::read_to_string(&a) {
        let linhas: Vec<&str> = texto.lines().collect();
        if linhas.len() > GUARDAR * 2 {
            let tmp = a.with_extension("tmp");
            if std::fs::write(&tmp, linhas[linhas.len() - GUARDAR..].join("\n") + "\n").is_ok() {
                let _ = std::fs::rename(tmp, &a);
            }
        }
    }
}

/// As mais recentes primeiro, sem repetir o mesmo SQL.
pub(crate) fn ler() -> Vec<Entrada> {
    let mut v = arquivo().and_then(|a| std::fs::read_to_string(a).ok()).map(|t| de_texto(&t)).unwrap_or_default();
    v.reverse();
    let mut vistos = std::collections::HashSet::new();
    v.retain(|e| vistos.insert(e.sql.clone()));
    v
}

/// "há 5 min", "há 2 h", "há 3 dias" (sem fuso horário para errar).
fn ha(quando: u64) -> String {
    let s = agora().saturating_sub(quando);
    match s {
        0..60 => tr("agora").to_owned(),
        60..3600 => trf("há {} min", &[&(s / 60)]),
        3600..86400 => trf("há {} h", &[&(s / 3600)]),
        _ => trf("há {} dias", &[&(s / 86400)]),
    }
}

impl App {
    pub(crate) fn view_historico<'a>(&'a self, base: Element<'a, Msg>) -> Element<'a, Msg> {
        let Some((busca, todas)) = &self.historico else { return base };
        let b = busca.trim().to_lowercase();
        let achadas: Vec<&Entrada> = todas.iter().filter(|e| b.is_empty() || e.sql.to_lowercase().contains(&b) || e.onde.to_lowercase().contains(&b)).take(MOSTRAR).collect();
        let campo = text_input(tr("Buscar no histórico"), busca).on_input(|t| Msg::Historico(Some(t))).autofocus(1).padding([7, 10]);
        let mut lista = column![].spacing(2);
        if achadas.is_empty() {
            lista = lista.push(text(tr("Nada rodado ainda.")).size(13).color(COR_TEXTO_MUTED()));
        }
        for e in achadas {
            let previa: String = e.sql.lines().take(3).collect::<Vec<_>>().join("\n").chars().take(240).collect();
            let corpo = column![text(previa).size(12.5).font(Font::MONO), text(format!("{} · {}", e.onde, ha(e.quando))).size(11.5).color(COR_TEXTO_MUTED())].spacing(3);
            lista = lista.push(button(corpo).padding([6, 8]).width(Length::FILL).style(estilo_botao_pequeno(false)).on_press(Msg::UsarHistorico(e.sql.clone())));
        }
        let corpo = column![campo, feather_ui::dialogo::rolagem(lista)].spacing(10);
        feather_ui::dialogo::Dialogo::new(tr("Histórico de consultas"), corpo)
            .icone("history")
            .subtitulo(tr("Clique para pôr no editor."))
            .largura(620.0)
            .altura_maxima(560.0)
            .ao_fechar(Msg::Historico(None))
            .secundario(tr("Fechar"), Some(Msg::Historico(None)))
            .sobre(base)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta_com_quebra_e_tabulacao() {
        let sql = "SELECT 'a\\tb'\n\tFROM t\\x";
        let e = de_texto(&linha(42, "Banco / x", sql));
        assert_eq!((e[0].quando, e[0].onde.as_str(), e[0].sql.as_str()), (42, "Banco / x", sql));
        assert!(de_texto("lixo\n").is_empty());
        assert!(tem_senha("ALTER USER app WITH PASSWORD 'x'") && tem_senha("create user a identified by 'x'") && !tem_senha("select 1"));
    }
}

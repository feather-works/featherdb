//! Preferências do FeatherDB: `config.txt` (linhas `chave=valor`) na pasta de configuração,
//! ao lado de `conexoes.txt` (as conexões, sem senha) e `rascunho.sql` (o texto do editor).
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tema {
    Automatico,
    Claro,
    Escuro,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Config {
    pub(crate) tema: Tema,
    /// Índice em `feather_ui::tema::ACENTOS`.
    pub(crate) acento: usize,
    pub(crate) idioma: String,
    /// A regra de boas-vindas da suíte (`feather_ui::novidades::ao_abrir`).
    pub(crate) primeiros_passos_vistos: bool,
    pub(crate) versao_vista: String,
}

impl Default for Config {
    fn default() -> Self {
        Config { tema: Tema::Automatico, acento: 0, idioma: crate::idioma::do_sistema().into(), primeiros_passos_vistos: false, versao_vista: String::new() }
    }
}

/// `~/.config/featherdb` no Linux, `%APPDATA%\featherdb` no Windows.
pub(crate) fn pasta() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("featherdb"))
}

impl Config {
    /// Lê as preferências; `existia` diz se o arquivo já estava lá (primeira vez = não).
    pub(crate) fn carregar() -> (Config, bool) {
        match pasta().and_then(|p| std::fs::read_to_string(p.join("config.txt")).ok()) {
            Some(texto) => (Config::de_texto(&texto), true),
            None => (Config::default(), false),
        }
    }

    fn de_texto(texto: &str) -> Config {
        let mut c = Config::default();
        for linha in texto.lines() {
            let Some((chave, valor)) = linha.split_once('=') else { continue };
            let valor = valor.trim();
            match chave.trim() {
                "tema" => {
                    c.tema = match valor {
                        "claro" => Tema::Claro,
                        "escuro" => Tema::Escuro,
                        _ => Tema::Automatico,
                    }
                }
                "acento" => c.acento = valor.parse().ok().filter(|i| *i < feather_ui::tema::ACENTOS.len()).unwrap_or(0),
                "idioma" if crate::idioma::LINGUAS.iter().any(|(l, _)| *l == valor) => c.idioma = valor.into(),
                "primeiros_passos_vistos" => c.primeiros_passos_vistos = valor == "1",
                "versao_vista" => c.versao_vista = valor.into(),
                _ => {}
            }
        }
        c
    }

    fn em_texto(&self) -> String {
        let tema = match self.tema {
            Tema::Automatico => "automatico",
            Tema::Claro => "claro",
            Tema::Escuro => "escuro",
        };
        format!(
            "tema={tema}\nacento={}\nidioma={}\nprimeiros_passos_vistos={}\nversao_vista={}\n",
            self.acento,
            self.idioma,
            u8::from(self.primeiros_passos_vistos),
            self.versao_vista
        )
    }

    /// Grava num arquivo ao lado e troca de uma vez (o anterior nunca fica pela metade).
    pub(crate) fn gravar(&self) -> Result<(), String> {
        let pasta = pasta().ok_or("pasta de configuração não encontrada")?;
        std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
        let temporario = pasta.join(".config.txt.tmp");
        std::fs::write(&temporario, self.em_texto()).map_err(|e| e.to_string())?;
        std::fs::rename(&temporario, pasta.join("config.txt")).map_err(|e| e.to_string())
    }

    /// Aplica tema, cor de destaque e idioma na interface inteira.
    pub(crate) fn aplicar(&self) {
        let escuro = feather_ui::tema::escuro_resolvido(self.tema == Tema::Automatico, self.tema == Tema::Escuro);
        feather_ui::tema::definir_tema(escuro, self.acento);
        crate::idioma::definir(&self.idioma);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta_pelo_texto_e_tolera_lixo() {
        let c = Config { tema: Tema::Escuro, acento: 2, idioma: "es-ES".into(), primeiros_passos_vistos: true, versao_vista: "0.1.0".into() };
        assert_eq!(Config::de_texto(&c.em_texto()), c);
        let estranho = Config::de_texto("tema=roxo\nacento=99\nidioma=klingon\nsem igual\n");
        assert_eq!(estranho, Config::default());
    }
}

//! O formulário de conexão (nova ou editar), com "Testar" antes de salvar.
use feather_ui::dialogo::Dialogo;
use feather_ui::tema::*;
use featherdb_core::{Conexao, Tipo};
use pelicano::{checkbox, column, pick_list, row, text, text_input, Align, Element, Length, Task};

use crate::idioma::tr;
use crate::{em_segundo_plano, App, Item, Msg};

pub(crate) struct Formulario {
    /// `None`: conexão nova.
    editando: Option<u64>,
    c: Conexao,
    porta: String,
    senha: String,
    testando: bool,
    /// Resultado do "Testar": ok e a mensagem.
    teste: Option<(bool, String)>,
}

#[derive(Clone, Debug)]
pub(crate) enum Campo {
    Tipo(Tipo),
    Nome(String),
    Host(String),
    Porta(String),
    Banco(String),
    Usuario(String),
    Senha(String),
    Caminho(String),
    Tls(bool),
    EscolherArquivo,
    Testar,
    Testado(Result<(), String>),
    Salvar,
    Cancelar,
}

/// Exemplos para a string ODBC (o mesmo papel do "driver" do DBeaver).
const EXEMPLOS_ODBC: &str = "DSN=meu_banco\nDriver={HDBODBC};ServerNode=<host>:443;Encrypt=TRUE   (SAP HANA / Datasphere)\nDriver={TDV 8.0};Host=<host>;Port=9401   (TIBCO TDV)\nDriver={ODBC Driver 18 for SQL Server};Server=<host>;Database=<banco>";

impl Formulario {
    pub(crate) fn novo(editando: Option<u64>, c: Conexao) -> Formulario {
        Formulario { editando, porta: c.porta.to_string(), c, senha: String::new(), testando: false, teste: None }
    }

    /// A conexão com a porta do campo; `Err` com o que está faltando.
    fn conexao(&self) -> Result<Conexao, &'static str> {
        let mut c = self.c.clone();
        match c.tipo {
            Tipo::Postgres | Tipo::MySql => {
                c.porta = self.porta.trim().parse().map_err(|_| tr("A porta precisa ser um número de 1 a 65535."))?;
                if c.porta == 0 {
                    return Err(tr("A porta precisa ser um número de 1 a 65535."));
                }
                if c.host.trim().is_empty() {
                    return Err(tr("Falta o servidor (host)."));
                }
            }
            Tipo::Sqlite if c.caminho.trim().is_empty() => return Err(tr("Escolha o arquivo do banco SQLite.")),
            Tipo::Odbc if c.caminho.trim().is_empty() => return Err(tr("Escreva a string de conexão ODBC.")),
            _ => {}
        }
        Ok(c)
    }

    pub(crate) fn view(&self) -> Dialogo<'_, Msg> {
        let f = |c: Campo| Msg::Form(c);
        let campo = |rotulo: &str, valor: &str, msg: fn(String) -> Campo| -> Element<'static, Msg> {
            column![text(rotulo.to_owned()).size(12).color(COR_TEXTO_MUTED()), text_input("", valor.to_owned()).on_input(move |s| Msg::Form(msg(s))).padding([6, 9]).size(13)]
                .spacing(4)
                .width(Length::FILL)
                .into()
        };
        let mut corpo = column![
            row![
                column![text(tr("Banco")).size(12).color(COR_TEXTO_MUTED()), pick_list(&Tipo::TODOS[..], Some(self.c.tipo), |t| Msg::Form(Campo::Tipo(t)))].spacing(4).width(200),
                campo(tr("Nome na lista (opcional)"), &self.c.nome, Campo::Nome),
            ]
            .spacing(12),
        ]
        .spacing(12);
        match self.c.tipo {
            Tipo::Postgres | Tipo::MySql => {
                corpo = corpo.push(row![
                    column![
                        text(tr("Servidor (host)")).size(12).color(COR_TEXTO_MUTED()),
                        text_input(tr("ou cole o endereço postgresql://…"), self.c.host.as_str()).on_input(|s| Msg::Form(Campo::Host(s))).padding([6, 9]).size(13),
                    ]
                    .spacing(4)
                    .width(Length::FILL),
                    column![text(tr("Porta")).size(12).color(COR_TEXTO_MUTED()), text_input("", self.porta.as_str()).on_input(|s| Msg::Form(Campo::Porta(s))).padding([6, 9]).size(13)].spacing(4).width(100)].spacing(12));
                corpo = corpo.push(campo(tr("Banco de dados"), &self.c.banco, Campo::Banco));
                corpo = corpo.push(row![campo(tr("Usuário"), &self.c.usuario, Campo::Usuario), self.campo_senha()].spacing(12));
                corpo = corpo.push(checkbox(tr("Conexão criptografada (TLS), exigida pela maioria dos bancos na nuvem"), self.c.tls).on_toggle(|v| Msg::Form(Campo::Tls(v))).size(13));
            }
            Tipo::Sqlite => {
                corpo = corpo.push(
                    row![campo(tr("Arquivo"), &self.c.caminho, Campo::Caminho), feather_ui::dialogo::botao_secundario(tr("Escolher…"), Some(f(Campo::EscolherArquivo)))]
                        .spacing(8)
                        .align(Align::End),
                );
                corpo = corpo.push(text(tr("Se o arquivo não existir, um banco novo e vazio é criado nele.")).size(12).color(COR_TEXTO_MUTED()));
            }
            Tipo::Odbc => {
                corpo = corpo.push(campo(tr("String de conexão"), &self.c.caminho, Campo::Caminho));
                corpo = corpo.push(text(tr("Instale o driver ODBC do fornecedor (SAP HANA para o Datasphere, TIBCO para o TDV…) e escreva a string. Exemplos:")).size(12).color(COR_TEXTO_MUTED()));
                corpo = corpo.push(text(EXEMPLOS_ODBC).size(11.5).font(pelicano::Font::MONO).color(COR_TEXTO_MUTED()));
                corpo = corpo.push(row![campo(tr("Usuário (opcional)"), &self.c.usuario, Campo::Usuario), self.campo_senha()].spacing(12));
            }
        }
        if let Some((ok, msg)) = &self.teste {
            corpo = corpo.push(text(msg.as_str()).size(12.5).color(if *ok { COR_AZUL_ACENTO() } else { COR_ERRO() }));
        }
        let titulo = if self.editando.is_some() { tr("Editar conexão") } else { tr("Nova conexão") };
        Dialogo::new(titulo, corpo)
            .icone("settings")
            .largura(620.0)
            .ao_fechar(f(Campo::Cancelar))
            .secundario(tr("Cancelar"), Some(f(Campo::Cancelar)))
            .secundario(if self.testando { tr("Testando…") } else { tr("Testar") }, (!self.testando).then_some(f(Campo::Testar)))
            .primario(tr("Salvar e conectar"), Some(f(Campo::Salvar)))
    }

    fn campo_senha(&self) -> Element<'_, Msg> {
        column![
            text(tr("Senha (não fica guardada)")).size(12).color(COR_TEXTO_MUTED()),
            text_input("", self.senha.as_str()).secure(true).on_input(|s| Msg::Form(Campo::Senha(s))).padding([6, 9]).size(13),
        ]
        .spacing(4)
        .width(Length::FILL)
        .into()
    }
}

impl App {
    pub(crate) fn update_form(&mut self, campo: Campo) -> Task<Msg> {
        let Some(form) = &mut self.form else { return Task::none() };
        match campo {
            Campo::Tipo(t) => {
                // Troca o tipo sem apagar o que já foi digitado; a porta segue o padrão do banco novo.
                let antes = form.c.tipo;
                form.c.tipo = t;
                if form.porta.trim() == antes.porta_padrao().to_string() || form.porta.trim() == "0" {
                    form.porta = t.porta_padrao().to_string();
                }
                if matches!(t, Tipo::Postgres | Tipo::MySql) && form.c.host.is_empty() {
                    form.c.host = "localhost".into();
                }
            }
            Campo::Nome(s) => form.c.nome = s,
            Campo::Host(s) => match Conexao::de_url(&s) {
                // Colou o endereço inteiro: preenche tudo de uma vez (o nome dado fica).
                Some((c, senha)) => {
                    form.c = Conexao { nome: std::mem::take(&mut form.c.nome), ..c };
                    form.porta = form.c.porta.to_string();
                    if !senha.is_empty() {
                        form.senha = senha;
                    }
                }
                None => form.c.host = s,
            },
            Campo::Porta(s) => form.porta = s,
            Campo::Banco(s) => form.c.banco = s,
            Campo::Usuario(s) => form.c.usuario = s,
            Campo::Senha(s) => form.senha = s,
            Campo::Caminho(s) => form.c.caminho = s,
            Campo::Tls(v) => form.c.tls = v,
            Campo::EscolherArquivo => {
                // Cancelar a escolha mantém o arquivo que já estava no campo.
                let atual = form.c.caminho.clone();
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new().add_filter("SQLite", &["db", "sqlite", "sqlite3", "db3"]).add_filter("*", &["*"]).pick_file().await.map(|a| a.path().to_string_lossy().into_owned())
                    },
                    move |c| Msg::Form(Campo::Caminho(c.unwrap_or(atual))),
                );
            }
            Campo::Testar => match form.conexao() {
                Err(e) => form.teste = Some((false, e.into())),
                Ok(c) => {
                    form.testando = true;
                    form.teste = None;
                    let senha = form.senha.clone();
                    return em_segundo_plano(move || c.conectar(&senha).map(|_| ()), |r| Msg::Form(Campo::Testado(r)));
                }
            },
            Campo::Testado(r) => {
                form.testando = false;
                form.teste = Some(match r {
                    Ok(()) => (true, tr("Conectou. Tudo certo.").into()),
                    Err(e) => (false, e),
                });
            }
            Campo::Salvar => {
                let c = match form.conexao() {
                    Ok(c) => c,
                    Err(e) => {
                        form.teste = Some((false, e.into()));
                        return Task::none();
                    }
                };
                let senha = (!form.senha.is_empty() || c.tipo == Tipo::Sqlite).then(|| form.senha.clone());
                let id = match form.editando {
                    Some(id) => {
                        if let Some(i) = self.item(id) {
                            // Mudou a conexão: fecha a antiga, conecta de novo com os dados novos.
                            *i = Item::novo(id, c);
                        }
                        id
                    }
                    None => {
                        let id = self.proximo_id;
                        self.proximo_id += 1;
                        self.itens.push(Item::novo(id, c));
                        id
                    }
                };
                if let Some(i) = self.item(id) {
                    i.senha = senha;
                }
                self.form = None;
                self.gravar_conexoes();
                return self.conectar(id);
            }
            Campo::Cancelar => self.form = None,
        }
        Task::none()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn trocar_o_tipo_nao_apaga_o_que_foi_digitado() {
        let mut app = App::novo(crate::config::Config::default());
        let mut c = Conexao::nova(Tipo::Postgres);
        c.host = "db.empresa.com".into();
        c.usuario = "ana".into();
        app.form = Some(Formulario::novo(None, c));
        let _ = app.update_form(Campo::Tipo(Tipo::MySql));
        let f = app.form.as_ref().unwrap();
        assert_eq!((f.c.host.as_str(), f.c.usuario.as_str(), f.porta.as_str()), ("db.empresa.com", "ana", "3306"));
    }

    #[test]
    fn colar_o_endereco_preenche_tudo() {
        let mut app = App::novo(crate::config::Config::default());
        app.form = Some(Formulario::novo(None, Conexao { nome: "Lotofácil".into(), ..Conexao::nova(Tipo::MySql) }));
        let _ = app.update_form(Campo::Host("postgresql://postgres:x@10.0.0.5:5433/loto".into()));
        let f = app.form.as_ref().unwrap();
        assert_eq!((f.c.tipo, f.c.nome.as_str(), f.c.host.as_str(), f.porta.as_str(), f.c.banco.as_str(), f.senha.as_str()), (Tipo::Postgres, "Lotofácil", "10.0.0.5", "5433", "loto", "x"));
    }

    #[test]
    fn porta_e_campos_obrigatorios() {
        let mut f = Formulario::novo(None, Conexao::nova(Tipo::Postgres));
        assert_eq!(f.conexao().unwrap().porta, 5432);
        f.porta = "abc".into();
        assert!(f.conexao().is_err());
        let f = Formulario::novo(None, Conexao::nova(Tipo::Sqlite));
        assert!(f.conexao().is_err());
    }
}

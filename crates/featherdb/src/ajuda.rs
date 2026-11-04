//! O conteúdo da Ajuda (F1), nos três idiomas. A janela e a busca são da `feather_ui::ajuda`.
//! Os ids evitam os da planilha (`comecar`, `formatar`…): a página inicial da feather-ui liga
//! atalhos de planilha a eles.
use std::sync::OnceLock;

use feather_ui::ajuda::{Catalogo, Linha, Secao, Topico};

use crate::idioma::tr;

/// Seção que a regra de boas-vindas abre na primeira vez.
pub(crate) const PRIMEIROS_PASSOS: &str = "primeiros-passos";

fn t(s: &str) -> Linha {
    Linha::Texto(tr(s).into())
}
fn m(s: &str) -> Linha {
    Linha::Marcador(tr(s).into())
}
fn a(descricao: &str, tecla: &str) -> Linha {
    Linha::Atalho { descricao: tr(descricao).into(), tecla: tecla.into() }
}
fn topico(id: &str, titulo: &str, linhas: Vec<Linha>) -> Topico {
    Topico { id: id.into(), titulo: tr(titulo).into(), linhas }
}
fn secao(id: &str, titulo: &str, introducao: &str, topicos: Vec<Topico>) -> Secao {
    Secao { id: id.into(), titulo: tr(titulo).into(), introducao: tr(introducao).into(), topicos }
}

/// O catálogo no idioma atual (montado uma vez por idioma).
pub(crate) fn catalogo() -> &'static Catalogo {
    static CATALOGOS: [OnceLock<Catalogo>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    CATALOGOS[crate::idioma::indice().min(2)].get_or_init(montar)
}

fn montar() -> Catalogo {
    Catalogo {
        titulo: tr("Ajuda do FeatherDB").into(),
        busca_rotulo: tr("Buscar na ajuda").into(),
        nada_encontrado: tr("Nada encontrado. Tente outra palavra.").into(),
        anterior: tr("Anterior").into(),
        proximo: tr("Próximo").into(),
        nesta_secao: tr("Nesta seção").into(),
        secoes: vec![
            secao(
                PRIMEIROS_PASSOS,
                "Primeiros passos",
                "O FeatherDB conversa com bancos de dados: você escreve SQL e vê o resultado numa grade. Leve, sem conta e sem telemetria.",
                vec![
                    // A primeira linha vira o resumo do cartão na página inicial da Ajuda: curta, para caber.
                    topico("primeira-conexao", "Sua primeira conexão", vec![
                        t("Conecte no seu banco em um minuto."),
                        t("Clique em Nova conexão (o + no alto), escolha o banco e preencha o servidor, o banco e o usuário."),
                        t("Testar confere sem salvar. Salvar e conectar guarda a conexão na lista à esquerda."),
                        t("Com Lembrar a senha ligado, ela fica no cofre de senhas do sistema (o mesmo do navegador) e não é pedida de novo. Nunca vai para um arquivo de texto."),
                    ]),
                    topico("primeira-consulta", "Sua primeira consulta", vec![
                        t("Escreva SQL e rode com Ctrl+Enter."),
                        t("Escreva o SQL no editor e aperte Ctrl+Enter: roda o comando onde está o cursor (entre os ponto e vírgula)."),
                        t("Com um trecho selecionado, roda só o trecho. Para rodar o script inteiro, Alt+X ou F5."),
                        a("Rodar o comando do cursor", "Ctrl+Enter"),
                        a("Rodar o script inteiro", "Alt+X"),
                    ]),
                ],
            ),
            secao("editor", "O editor de SQL", "Escrever, rodar e parar consultas com segurança.", vec![
                topico("abas-consulta", "Abas de consulta", vec![
                    t("Cada aba tem o seu SQL, a sua conexão e o seu resultado: dá para deixar produção numa e homologação na outra."),
                    t("O texto de cada aba fica guardado e volta quando você abrir o FeatherDB de novo."),
                    a("Nova aba", "Ctrl+T"),
                    a("Fechar a aba", "Ctrl+W"),
                ]),
                topico("completar", "Completar nomes", vec![
                    t("Ao digitar, aparecem as tabelas e colunas do banco que começam com o que você escreveu. Depois de tabela. aparecem só as colunas dela."),
                    t("As setas escolhem, Tab ou Enter põe o nome (com aspas quando precisa) e Esc fecha a lista."),
                ]),
                topico("parar-e-avisos", "Parar e avisos", vec![
                    t("Enquanto a consulta roda, o botão Executar vira Parar e o banco interrompe a consulta."),
                    t("Um DELETE ou UPDATE sem WHERE mudaria a tabela inteira: o FeatherDB mostra o comando e pergunta antes de rodar."),
                    t("Se a conexão caiu (servidor reiniciou, computador ficou parado), ela é refeita sozinha. Uma consulta de leitura roda de novo; um comando que muda dados não, para não rodar duas vezes."),
                ]),
                topico("formatar-historico", "Formatar, plano e histórico", vec![
                    t("A varinha formata o SQL (palavras-chave em maiúscula, uma cláusula por linha). Com um trecho selecionado, formata só ele. Ctrl+Z desfaz."),
                    t("O botão do plano mostra o EXPLAIN do comando do cursor (PostgreSQL, MySQL e SQLite): por onde o banco vai ler os dados."),
                    t("O histórico guarda as consultas rodadas, com a conexão e quando. Clique numa para pôr no editor."),
                    a("Formatar o SQL", "Ctrl+Shift+F"),
                    a("Histórico de consultas", "Ctrl+H"),
                ]),
                topico("arquivos-sql", "Abrir e salvar .sql", vec![
                    t("A pasta abre um arquivo .sql: numa aba vazia ele entra nela, senão abre numa aba nova com o nome do arquivo."),
                    t("O disquete salva o SQL da aba no arquivo. Na primeira vez pergunta onde; depois Ctrl+S grava no mesmo arquivo."),
                    a("Abrir arquivo .sql", "Ctrl+O"),
                    a("Salvar", "Ctrl+S"),
                    a("Salvar como", "Ctrl+Shift+S"),
                ]),
                topico("views-funcoes", "Views, views materializadas e funções", vec![
                    t("Botão direito na conexão ou no banco: Nova view, Nova view materializada (PostgreSQL) e Nova função abrem um modelo numa aba nova. Se o comando do cursor for um SELECT, a view já vem com ele."),
                    t("Uma view é uma consulta guardada com nome: toda vez que você lê, ela roda de novo. A view materializada guarda o resultado: ler é rápido, mas os dados só mudam com Atualizar os dados (REFRESH), no botão direito dela."),
                    t("As funções do banco (PostgreSQL e MySQL) aparecem na árvore, embaixo das tabelas. Clique numa para abrir o código numa aba e mudar."),
                ]),
            ]),
            secao("bancos", "Bancos de dados", "PostgreSQL, MySQL, SQLite e MongoDB vêm prontos. Os outros entram por ODBC.", vec![
                topico("postgres-mysql", "PostgreSQL, MySQL e MariaDB", vec![
                    t("Preencha servidor, porta, banco e usuário. Bancos na nuvem quase sempre exigem a opção de conexão criptografada (TLS)."),
                    t("Deixou o banco em branco? A árvore mostra todos os bancos do servidor; clique num deles para abrir as tabelas."),
                    t("Servidor interno com certificado próprio? Ligue o TLS e desligue Conferir o certificado (só criptografa), ou escolha o arquivo da autoridade (CA) da empresa. Se o servidor pede certificado do cliente, escolha o certificado e a chave."),
                    t("Tem o endereço inteiro (postgresql://usuario:senha@servidor:5432/banco)? Cole no campo Servidor: os outros campos se preenchem sozinhos."),
                ]),
                topico("sqlite", "SQLite", vec![
                    t("Escolha o arquivo do banco. Se ele não existir, o FeatherDB cria um banco novo e vazio."),
                ]),
                topico("mongodb", "MongoDB e Atlas", vec![
                    t("Cole o endereço mongodb:// ou mongodb+srv:// (o do Atlas, em Connect › Drivers). A senha é a do usuário do banco, cadastrada em Database Access, e não a do site."),
                    t("No editor vão os comandos do shell do Mongo: db.clientes.find({idade: {$gt: 30}}).sort({nome: 1}).limit(10), aggregate([...]), countDocuments({...}), show collections e show dbs."),
                    t("Cada documento vira uma linha e cada campo do primeiro nível, uma coluna; objetos e listas aparecem como JSON."),
                ]),
                topico("odbc", "Datasphere, TDV e outros (ODBC)", vec![
                    t("Para bancos que não vêm prontos, instale o driver ODBC do fornecedor e escreva a string de conexão. É o mesmo papel do driver JDBC do DBeaver, sem precisar de Java."),
                    m("SAP Datasphere e HANA: driver HDBODBC (SAP HANA Client)."),
                    m("TIBCO Data Virtualization (TDV): o driver ODBC do TDV."),
                    m("SQL Server, Oracle, Snowflake, Db2: o driver ODBC de cada um."),
                    t("No Linux, o gerenciador ODBC é o pacote unixodbc. No Windows ele já vem com o sistema."),
                ]),
            ]),
            secao("navegar", "Tabelas e resultados", "A árvore à esquerda mostra o que existe no banco.", vec![
                topico("arvore", "A árvore de tabelas", vec![
                    t("Clique numa conexão para conectar e abrir as tabelas. Clique numa tabela para ver as colunas e o tipo de cada uma."),
                    t("Clique duplo numa tabela mostra os dados dela na grade."),
                    t("Botão direito numa tabela: ver os dados, escrever o SELECT com as colunas no editor, contar as linhas ou copiar o nome. Na conexão: conectar, editar e excluir."),
                    t("Criou uma tabela? O botão Atualizar tabelas lê a lista de novo."),
                    t("Muitas tabelas? Escreva parte do nome em Buscar tabela ou coluna, no alto da árvore."),
                ]),
                topico("grade", "A grade do resultado", vec![
                    t("A grade traz até mil linhas e rola leve, porque só desenha o que aparece na tela. Para ver outras, use WHERE, ORDER BY ou LIMIT."),
                    t("Clique no nome de uma coluna para ordenar (de novo inverte, a terceira volta à ordem do banco). Filtrar linhas esconde as que não têm o texto."),
                    t("Copiar tudo põe o resultado na área de transferência, pronto para colar no FeatherSheet ou no Excel. Exportar CSV grava um arquivo."),
                ]),
                topico("estrutura", "Estrutura das tabelas", vec![
                    t("A aba Estrutura mostra cada coluna com o tipo, se aceita vazio, o valor padrão e a chave. O lápis edita a coluna e mostra o SQL antes de aplicar."),
                    t("A busca da lateral acha tabelas e colunas pelo nome, em todos os bancos abertos; clicar numa coluna abre a estrutura da tabela dela."),
                    t("Para ter a lista de colunas fora do FeatherDB: copie ou exporte as colunas de uma tabela, ou exporte o dicionário do banco inteiro (botão direito no banco ou na conexão)."),
                ]),
                topico("editar-dados", "Editar os dados", vec![
                    t("Na tabela aberta com clique duplo (PostgreSQL, MySQL e SQLite, com chave primária), clique duas vezes numa célula para mudar o valor. Enter guarda, Esc desiste e o botão NULL esvazia."),
                    t("As células mudadas ficam marcadas. Salvar mostra os UPDATE antes de gravar; se um falhar, nenhum fica gravado."),
                ]),
                topico("sql-da-tabela", "O SQL da tabela", vec![
                    t("A aba SQL mostra o CREATE TABLE (ou CREATE VIEW) da tabela aberta, para copiar ou pôr no editor."),
                ]),
                topico("rascunho", "Seu SQL fica guardado", vec![
                    t("O texto do editor é guardado ao rodar e ao fechar, e volta na próxima vez que você abrir o FeatherDB."),
                ]),
            ]),
        ],
    }
}

#[cfg(test)]
mod testes {
    #[test]
    fn primeira_secao_e_os_primeiros_passos_e_os_ids_nao_repetem() {
        let c = super::catalogo();
        assert_eq!(c.secoes[0].id, super::PRIMEIROS_PASSOS);
        let mut ids: Vec<&str> = c.secoes.iter().flat_map(|s| s.topicos.iter().map(|t| t.id.as_str())).collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total);
    }

    #[test]
    fn todo_texto_da_ajuda_tem_traducao() {
        use feather_ui::ajuda::Linha;
        let c = super::montar();
        let mut textos = vec![c.titulo, c.busca_rotulo, c.nada_encontrado, c.anterior, c.proximo, c.nesta_secao];
        for s in c.secoes {
            textos.push(s.titulo);
            textos.push(s.introducao);
            for t in s.topicos {
                textos.push(t.titulo);
                for l in t.linhas {
                    match l {
                        Linha::Texto(x) | Linha::Marcador(x) | Linha::Exemplo(x) | Linha::Atalho { descricao: x, .. } => textos.push(x),
                    }
                }
            }
        }
        let faltam: Vec<_> = textos.iter().filter(|t| !crate::idioma::sem_traducao(t).is_empty()).collect();
        assert!(faltam.is_empty(), "sem tradução: {faltam:#?}");
    }

    /// A página inicial da Ajuda mostra a primeira linha de cada tópico dos Primeiros passos num
    /// cartão estreito; acima de ~42 caracteres o texto passa da borda do cartão.
    #[test]
    fn resumos_dos_cartoes_cabem() {
        let c = super::montar();
        for t in &c.secoes[0].topicos {
            let Some(feather_ui::ajuda::Linha::Texto(x)) = t.linhas.first() else { panic!("{} sem texto primeiro", t.id) };
            assert!(x.chars().count() <= 42, "{}: {x}", t.id);
        }
    }
}

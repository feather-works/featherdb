# FeatherDB

Regras gerais da suíte: `../FeatherOffice/CLAUDE.md` (pt-BR, privacidade, nada de copiar código de
outro projeto) e `../feather-ui-pelicano/GUIA-JANELAS.md` (ícones só Tabler, cores só pelos tokens de
`feather_ui::tema`, nada de emoji).

- Cliente de banco de dados leve (a ideia do DBeaver), criado em 04/11/2026 a pedido do usuário.
- Interface em **pelicano** + `feather-ui` (tags publicadas, sem `[patch]`).
- `featherdb-core`: conexões (`Conexao`, `ler_conexoes`/`gravar_conexoes`), o trait `Banco` e os drivers:
  Postgres (`postgres`, protocolo simples: tudo vem como texto; TLS com rustls), MySQL (`mysql`, 100% Rust),
  SQLite (`rusqlite` com o C embutido, exceção à regra "sem C" aceita pelo usuário) e ODBC
  (`odbc.rs`: a API ODBC carregada com `libloading` só ao conectar; o app abre sem unixODBC; funções "W",
  texto em UTF-16, para os acentos não quebrarem no Windows).
  ODBC é o caminho para Datasphere/HANA, TIBCO TDV e outros (o papel do JDBC no DBeaver, sem Java).
  MongoDB (`mongo_banco.rs`, driver oficial `mongodb` com `sync`, pedido do usuário em 04/11/2026): o endereço
  inteiro fica em `caminho` sem a senha; o editor aceita `db.x.find/aggregate/countDocuments` e `show
  collections/dbs` (argumentos em JSON5, `ObjectId(...)`/`ISODate(...)` viram JSON estendido).
- `Content::cursor()` do pelicano conta de 1; o motor (`comando_no_cursor`) conta de 0 (`App::comando_atual`).
- Árvore: `Item` (conexão) › `Base` (um banco aberto) › tabelas › colunas. Postgres/MySQL com o banco em branco
  listam os bancos (`Banco::bancos`) e cada um abre a sua conexão (`App::abrir_banco`); o `Alvo` (id, banco)
  diz onde o SQL roda.
- Visão da tabela (`estrutura_ui.rs`): abas Dados, Estrutura e SQL (`Banco::ddl`); `Coluna` (tipo, vazio, padrão, chave) e o
  dicionário (`Banco::dicionario`) vêm do motor; o SQL de editar/criar coluna sai de `estrutura::sql_alterar`/
  `sql_adicionar` e aparece no diálogo antes de rodar. A busca da lateral acha colunas pelo dicionário.
- A grade (`grade.rs`) monta só as linhas e colunas visíveis (mesma técnica do `examples/csv.rs` do pelicano):
  não voltar a desenhar todas as linhas, foi o que deixou o app lento no PC do usuário (04/11/2026).
- A interface só chama o motor; consultas rodam em `spawn_blocking` (a janela nunca trava).
- Rodar (`App::rodar`): DELETE/UPDATE sem WHERE pede confirmação (`featherdb_core::sem_where`); `Banco::cancelador`
  (Postgres por token, MySQL por `KILL QUERY`, SQLite por interrupt) é pego antes da consulta travar a conexão; sem
  cancelador (ODBC, Mongo) o Parar larga a conexão e abre outra. `rodada` descarta resposta velha. Conexão caída
  (`conexao_caiu`) é refeita dentro da tarefa; só leitura (`so_le`) roda de novo. Bloco `BEGIN … COMMIT` que falha
  leva `ROLLBACK` (usado ao salvar células).
- Abas (`abas.rs`): a aba da tela mora nos campos do `App`; as outras ficam em `consultas` e trocam com
  `App::trocar`. Não troca com consulta rodando (a resposta iria para a aba errada). Rascunhos `rascunho.sql`,
  `rascunho-2.sql`…; histórico em `historico.txt` (`historico.rs`, só SQL).
- Grade: ordenar/filtrar viram uma lista de índices (`visiveis`, `grade::ordem`); editar células (`celulas.rs`) só
  com chave primária no resultado, UPDATE de `estrutura::sql_atualizar` (MySQL dobra a `\`).
- Completar (`completar.rs`): tabelas e colunas da base ativa (dicionário lido na primeira vez); aceitar troca o
  começo da linha até o cursor com `Content::edit_lines`. Formatar: `featherdb_core::formatar`.
- TLS (`featherdb-core/src/tls.rs`): conferir (raízes webpki + CA escolhida) ou só criptografar (verificador que
  aceita o certificado); certificado e chave do cliente em PEM. Campos 10 a 13 de `conexoes.txt`.
- SQL Server nativo ficou de fora: o `tiberius` puxa `aws-lc` (C, cmake) pelo `tokio-rustls`. Hoje é por ODBC.
- A senha nunca vai para arquivo: "Lembrar a senha" (ligado por padrão, pedido do usuário em 04/11/2026)
  guarda no cofre do sistema (`featherdb-core::cofre`, crate `keyring`; Secret Service no Linux), só depois
  de conectar com sucesso. Excluir a conexão ou desligar o lembrar apaga do cofre. Arquivos em `~/.config/featherdb/`: `config.txt`, `conexoes.txt`, `rascunho.sql`.
- Idiomas: o texto em português no código é a chave; `idiomas/interface.{en-US,es-ES}.json`. Os testes
  conferem que todo `tr`/`trf` literal, a Ajuda e as Novidades têm tradução nas duas línguas.
- Marca: a pena da suíte recolorida em `#0b7285` dentro do app (`main.rs::marca`); passar para
  `feather_ui::marca::Produto` quando a feather-ui ganhar o FeatherDB.
- Testes: SQLite de ponta a ponta; ODBC contra o unixODBC (erro `IM002` de DSN inexistente); Postgres de
  verdade com `FEATHERDB_TESTE_PG=postgresql://… cargo test -p featherdb-core -- --ignored` (passou em
  04/11/2026 no Postgres da rede do usuário). A senha só por variável de ambiente, nunca em arquivo.
- Visual: editor e resultado em painéis com borda; a faixa do editor mostra a conexão ativa e o Executar.
  Conferir por captura (`pelicano::capture` num teste temporário) antes de entregar.

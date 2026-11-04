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
- `Content::cursor()` do pelicano conta de 1; o motor (`comando_no_cursor`) conta de 0 (`App::comando_atual`).
- A interface só chama o motor; consultas rodam em `spawn_blocking` (a janela nunca trava).
- A senha nunca vai para o disco. Arquivos em `~/.config/featherdb/`: `config.txt`, `conexoes.txt`, `rascunho.sql`.
- Idiomas: o texto em português no código é a chave; `idiomas/interface.{en-US,es-ES}.json`. Os testes
  conferem que todo `tr`/`trf` literal, a Ajuda e as Novidades têm tradução nas duas línguas.
- Marca: a pena da suíte recolorida em `#0b7285` dentro do app (`main.rs::marca`); passar para
  `feather_ui::marca::Produto` quando a feather-ui ganhar o FeatherDB.
- Testes: SQLite de ponta a ponta; ODBC contra o unixODBC (erro `IM002` de DSN inexistente); Postgres de
  verdade com `FEATHERDB_TESTE_PG=postgresql://… cargo test -p featherdb-core -- --ignored` (passou em
  04/11/2026 no Postgres da rede do usuário). A senha só por variável de ambiente, nunca em arquivo.
- Visual: editor e resultado em painéis com borda; a faixa do editor mostra a conexão ativa e o Executar.
  Conferir por captura (`pelicano::capture` num teste temporário) antes de entregar.

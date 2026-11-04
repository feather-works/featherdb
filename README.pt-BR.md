# FeatherDB

Cliente de banco de dados leve da suíte [FeatherWorks](https://github.com/feather-works), em Rust com o
[pelicano](https://github.com/feather-works/pelicano) e a [feather-ui](https://github.com/feather-works/feather-ui).
A ideia do DBeaver, sem o peso: abre na hora e usa cerca de 100 MB de memória.

[English](README.md) · [Español](README.es-ES.md)

![FeatherDB](docs/featherdb.png)

## O que faz

- **Conexões:** PostgreSQL, MySQL/MariaDB e SQLite prontos (TLS para bancos na nuvem). Qualquer outro
  banco por **ODBC**: SAP Datasphere/HANA, TIBCO TDV, SQL Server, Oracle, Snowflake… É o papel dos
  drivers JDBC do DBeaver, sem Java. O gerenciador ODBC só é carregado ao conectar: o FeatherDB abre
  mesmo sem o unixODBC instalado.
- **Editor de SQL:** `Ctrl+Enter` roda o comando do cursor (ou a seleção); `Alt+X`/`F5` roda o script
  inteiro. As consultas rodam em segundo plano: a janela nunca trava.
- **Árvore de tabelas:** tabelas, visões e colunas com o tipo; clique duplo numa tabela mostra os dados.
  Um filtro acha a tabela em bancos com milhares delas.
- **Resultado:** até 1.000 linhas, em páginas de 200; copiar tudo para colar no FeatherSheet ou no
  Excel, ou exportar CSV.
- **Privado:** sem conta, sem telemetria. A senha nunca vai para o disco; as conexões ficam em
  `~/.config/featherdb/conexoes.txt` (sem senha) e o texto do editor em `rascunho.sql`.
- Ajuda (F1), Configurações (tema, cor de destaque, idioma) e Novidades, em português, inglês e espanhol.

## Compilar

```sh
cargo run --release
```

## Licença

GPL-3.0.

# FeatherDB

Cliente de bases de datos ligero de la suite [FeatherWorks](https://github.com/feather-works), en Rust con
[pelicano](https://github.com/feather-works/pelicano) y [feather-ui](https://github.com/feather-works/feather-ui).
La idea de DBeaver, sin el peso: abre al instante y usa unos 100 MB de memoria.

[English](README.md) · [Português](README.pt-BR.md)

![FeatherDB](docs/featherdb.png)

## Qué hace

- **Conexiones:** PostgreSQL, MySQL/MariaDB y SQLite incluidos (TLS para bases en la nube, con "solo cifrar" para servidores internos, CA de la empresa y certificado del cliente). Pegue una
  dirección `postgresql://` o `mysql://` y el formulario se completa solo. Cualquier
  otra base por **ODBC**. **MongoDB** y Atlas (`mongodb+srv://`): colecciones en el árbol, `find`, `aggregate` y
  `countDocuments` en el editor, documentos como filas: SAP Datasphere/HANA, TIBCO TDV, SQL Server, Oracle, Snowflake… Cumple el papel
  de los drivers JDBC de DBeaver, sin Java. El administrador ODBC se carga solo al conectar: FeatherDB
  abre aunque unixODBC no esté instalado.
- **Editor SQL:** `Ctrl+Enter` ejecuta la sentencia del cursor (o la selección); `Alt+X`/`F5` ejecuta
  todo el script. Las consultas corren en segundo plano: la ventana nunca se congela, y **Detener** las cancela.
  Pestañas de consulta (`Ctrl+T`), cada una con su SQL, conexión y resultado; nombres de tablas y columnas completados
  al escribir; formatear el SQL (`Ctrl+Shift+F`); plan de la consulta (EXPLAIN); historial (`Ctrl+H`). Un `DELETE` o
  `UPDATE` sin `WHERE` pregunta antes de ejecutarse. La conexión caída se rehace sola.
- **Árbol de tablas:** sin base elegida, lista las bases del servidor (como DBeaver); tablas, vistas y columnas con su tipo; doble clic en una tabla muestra los datos;
  el clic derecho tiene SELECT, COUNT, copiar el nombre, editar y eliminar.
  Un filtro encuentra la tabla en bases con miles de ellas.
- **Vista de la tabla:** pestañas Datos, Estructura y SQL (el `CREATE TABLE`); doble clic en una celda edita los datos
  (tablas con clave primaria; Guardar muestra los `UPDATE` y graba todo o nada); cada columna muestra su tipo (con ícono por familia), si
  acepta vacío, el predeterminado y la clave. Editar o crear columnas viendo el SQL antes. Búsqueda de tablas y
  columnas por nombre en las bases abiertas; exportar las columnas de una tabla o el diccionario de toda la base.
- **Resultado:** hasta 1.000 filas que se desplazan livianas (solo se dibuja lo que aparece en pantalla); clic en una columna ordena, filtrar filas; copiar todo para pegar en FeatherSheet o Excel,
  o exportar CSV.
- **Privado:** sin cuenta, sin telemetría. La contraseña solo va al almacén de contraseñas del sistema (GNOME Keyring/KWallet,
  Administrador de credenciales de Windows), nunca a un archivo; las conexiones quedan en `~/.config/featherdb/conexoes.txt` el texto del editor en `rascunho.sql` y el historial en `historico.txt`.
- Ayuda (F1), Configuración (tema, color de acento, idioma) y Novedades, en portugués, inglés y español.

## Compilar

```sh
cargo run --release
```

## Licencia

GPL-3.0.

-- Módulo FICHA_LIBRO (rubro Librería): los datos de un producto que es
-- libro. El ISBN va como código del producto (es el código de barras del
-- libro), así que se escanea como cualquier otro.
--
-- Solo ADITIVO: una tabla nueva. No toca productos, ventas ni stock. Un
-- producto sin fila aquí es un producto común (en una librería también se
-- venden cuadernos, lapiceros y fotocopias).
--
--   producto_libro : una fila por libro.
--     autor     : "Mario Vargas Llosa" (varios: "Autor 1; Autor 2").
--     editorial : "Alfaguara".
--     serie     : saga o colección ("Harry Potter", "Biblioteca Escolar").
--     anio      : año de publicación (NULL = no se sabe).
--     edicion   : "2.ª edición", "Edición de bolsillo".

CREATE TABLE IF NOT EXISTS producto_libro (
  producto_id INTEGER PRIMARY KEY,
  autor TEXT,
  editorial TEXT,
  serie TEXT,
  anio INTEGER,
  edicion TEXT,
  fecha_actualizacion TEXT
);

CREATE INDEX IF NOT EXISTS idx_libro_editorial ON producto_libro (editorial);

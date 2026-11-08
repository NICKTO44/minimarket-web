-- Foto 2 del producto (módulo CATALOGO).
--
-- Pedido de PFASA (repuestos): en el Catálogo el vendedor quiere ver una
-- segunda foto de cada producto (otro ángulo, la caja, el n.° de parte).
-- En la tarjeta se pasa de una foto a otra con puntitos.
--
-- Solo ADITIVO: una tabla nueva. No toca productos ni su imagen_url (la
-- foto 1 sigue igual). Un producto sin fila aquí tiene solo su foto de
-- siempre. "orden" deja lugar para una foto 3 más adelante; hoy solo se usa 2.
--
-- El archivo vive junto a la foto 1: uploads/productos/{tienda}/{id}_2.jpg
-- (y su miniatura {id}_2_m.jpg).

CREATE TABLE IF NOT EXISTS producto_imagenes (
  producto_id INTEGER NOT NULL,
  orden INTEGER NOT NULL,
  url TEXT NOT NULL,
  fecha_actualizacion TEXT,
  PRIMARY KEY (producto_id, orden)
);

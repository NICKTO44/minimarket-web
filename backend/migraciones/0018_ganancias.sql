-- Reporte de ganancias (módulo GANANCIAS, apagado en todos los negocios).
--
-- Solo ADITIVO: dos columnas y una tabla nueva. Un negocio sin el módulo no
-- las usa: quedan en NULL / vacías y todo funciona igual que antes.
--
--   detalles_venta.costo_unitario : lo que costaba el producto al momento de
--     venderlo (con IGV). Queda congelado en la venta, así una compra
--     posterior más cara no cambia la ganancia de los meses pasados.
--     NULL = venta anterior al módulo, o producto que no tenía costo.
--
--   productos.costo_promedio : costo promedio ponderado del stock que hay en
--     la tienda. Se recalcula cuando entra mercadería (desde el producto,
--     el modelo con tallas, un lote o una compra a proveedor).
--     NULL = todavía no se calculó: vale el precio de compra del producto.
--     productos.precio_compra sigue siendo, como siempre, el último precio
--     que se pagó.
--
--   entradas_stock : cada ingreso de mercadería con lo que costó. De aquí
--     sale "Compras del mes" en el reporte. La fecha se guarda en UTC, igual
--     que las ventas; el reporte la pasa a hora de Perú.
--
-- La columna de productos va al final a propósito: si existe, todo lo
-- anterior de este archivo ya se aplicó.

ALTER TABLE detalles_venta ADD COLUMN costo_unitario REAL;

CREATE TABLE IF NOT EXISTS entradas_stock (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  producto_id INTEGER NOT NULL,
  cantidad REAL NOT NULL,
  costo_unitario REAL NOT NULL,
  -- PRODUCTO (formulario del producto), MODELO (modelo con tallas),
  -- LOTE (lote con vencimiento), COMPRA (recepción de una compra)
  origen TEXT NOT NULL,
  referencia_id INTEGER,
  fecha_hora TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (producto_id) REFERENCES productos(id)
);

CREATE INDEX IF NOT EXISTS idx_entradas_stock_fecha ON entradas_stock(fecha_hora);

ALTER TABLE productos ADD COLUMN costo_promedio REAL;

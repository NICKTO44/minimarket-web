-- IGV configurable: exonerado/inafecto por categoría o producto, y tasa
-- del negocio congelada en cada venta.
--
-- Solo ADITIVO. Todo en NULL = como antes: todo gravado a la tasa del
-- negocio (configuracion_tienda.iva_porcentaje, 18 por defecto).
--   categorias.afectacion_igv     : 'GRAVADO' | 'EXONERADO' | 'INAFECTO' (NULL = GRAVADO)
--   productos.afectacion_igv      : igual; NULL = el producto hereda el de su categoría
--   detalles_venta.afectacion_igv : cómo se vendió esa línea (congelado al cobrar)
--   ventas.igv_tasa               : tasa del negocio el día de la venta (congelada)
-- Se congelan en la venta para que un comprobante emitido o reimpreso
-- después salga igual que el día en que se vendió, aunque luego cambie la
-- categoría o la tasa.

ALTER TABLE categorias ADD COLUMN afectacion_igv TEXT;
ALTER TABLE productos ADD COLUMN afectacion_igv TEXT;
ALTER TABLE detalles_venta ADD COLUMN afectacion_igv TEXT;
ALTER TABLE ventas ADD COLUMN igv_tasa REAL;

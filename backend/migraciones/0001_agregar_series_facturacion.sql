-- Series de boleta/factura configurables por tenant. Antes vivían fijas
-- en facturacion.rs ("B001"/"F001" para todos por igual) — ahora cada
-- negocio puede tener las suyas propias, según lo que tenga realmente
-- activo en su cuenta de FacturaLibre. El default reproduce el valor
-- que todos los tenants ya tenían de forma implícita, así que aplicar
-- esta migración no cambia nada hasta que alguien la edite a mano.
ALTER TABLE configuracion_tienda ADD COLUMN serie_boleta TEXT DEFAULT 'B001';
ALTER TABLE configuracion_tienda ADD COLUMN serie_factura TEXT DEFAULT 'F001';
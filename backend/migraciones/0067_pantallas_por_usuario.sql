-- Pantallas por usuario (logica/pantallas.rs). Solo agrega.
-- JSON con las pantallas que el administrador le OCULTÓ al usuario,
-- por ejemplo ["REPORTES","HISTORIAL_CAJA"]. NULL = ve lo de su rol,
-- como siempre: los usuarios que ya existen no cambian.
ALTER TABLE usuarios ADD COLUMN pantallas_ocultas TEXT;

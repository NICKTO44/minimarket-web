-- Reintento automático de los comprobantes que quedaron PENDIENTES en la
-- emisión directa a SUNAT (sin conexión, SUNAT caído, error temporal).
--
-- Solo ADITIVO: dos columnas nuevas. Los comprobantes de FacturaLibre no
-- las usan.
--
--   intentos       : cuántas veces se volvió a enviar (además del primero).
--                    Sirve para espaciar los reintentos: no se golpea a
--                    SUNAT cada 10 minutos con algo que sigue fallando.
--   ultimo_intento : hora de Perú ("AAAA-MM-DD HH:MM:SS") del último envío.

ALTER TABLE comprobantes_electronicos ADD COLUMN intentos INTEGER NOT NULL DEFAULT 0;
ALTER TABLE comprobantes_electronicos ADD COLUMN ultimo_intento TEXT;

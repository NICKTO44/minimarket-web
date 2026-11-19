-- Módulo "Avisos de Yape y Plin" (AVISOS_PAGO).
--
-- avisos_dispositivos: celulares vinculados con la app Monspeet. Cada uno
-- tiene su propia clave (se guarda solo su huella SHA-256) y un modo:
--   YAPE   = el celular donde llegan los Yape/Plin (manda las notificaciones)
--   TIENDA = el celular del que atiende (escucha y avisa en voz alta)
CREATE TABLE IF NOT EXISTS avisos_dispositivos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  nombre TEXT NOT NULL,
  modo TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  creado_por INTEGER,
  creado_en TEXT NOT NULL DEFAULT (datetime('now')),
  ultimo_uso TEXT,
  activo INTEGER NOT NULL DEFAULT 1
);

-- pagos_recibidos: cada notificación de pago que mandó el celular del Yape.
-- reconocido = 0 si el texto no parece un pago recibido (se guarda igual,
-- para revisar). huella evita guardar dos veces la misma notificación.
CREATE TABLE IF NOT EXISTS pagos_recibidos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dispositivo_id INTEGER,
  billetera TEXT NOT NULL,
  monto REAL,
  nombre TEXT,
  codigo_seguridad TEXT,
  reconocido INTEGER NOT NULL DEFAULT 1,
  paquete TEXT,
  texto TEXT NOT NULL,
  huella TEXT NOT NULL UNIQUE,
  recibido_en TEXT NOT NULL DEFAULT (datetime('now')),
  venta_id INTEGER
);

CREATE INDEX IF NOT EXISTS idx_pagos_recibidos_fecha ON pagos_recibidos (recibido_en);

-- Amplía clientes.tipo_documento para aceptar Carnet de Extranjería y
-- Pasaporte, además de DNI/RUC/SIN_DOCUMENTO. Preserva los datos existentes.

CREATE TABLE clientes_nueva (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tipo_documento TEXT NOT NULL DEFAULT 'DNI' CHECK (tipo_documento IN ('DNI', 'CE', 'PASAPORTE', 'RUC', 'SIN_DOCUMENTO')),
  numero_documento TEXT,
  nombre_razon_social TEXT NOT NULL,
  telefono TEXT,
  email TEXT,
  direccion TEXT,
  activo INTEGER DEFAULT 1,
  fecha_creacion TEXT DEFAULT (datetime('now', 'localtime')),
  fecha_actualizacion TEXT DEFAULT (datetime('now', 'localtime'))
);

INSERT INTO clientes_nueva
SELECT id, tipo_documento, numero_documento, nombre_razon_social, telefono, email, direccion, activo, fecha_creacion, fecha_actualizacion
FROM clientes;

DROP TABLE clientes;
ALTER TABLE clientes_nueva RENAME TO clientes;

CREATE INDEX idx_clientes_documento ON clientes(numero_documento);
CREATE INDEX idx_clientes_nombre ON clientes(nombre_razon_social);

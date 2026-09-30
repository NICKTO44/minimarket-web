import { useState } from 'react';
import { Check, Minus, Plus } from 'lucide-react';

/**
 * Elegir opciones de un producto antes de agregarlo (Tamaño, Tipo de
 * leche, Extras...). El precio que se muestra es solo referencial: el
 * servidor vuelve a calcularlo con los precios reales.
 */
export default function ModalOpciones({ producto, grupos, onAgregar, onCerrar }) {
  // grupoId -> [opcionId]
  const [elegidas, setElegidas] = useState(() => {
    const inicial = {};
    grupos.forEach((g) => {
      // Si es obligatorio y de una sola opción, se preselecciona la primera
      // (normalmente la "normal": Chico, Leche entera...).
      inicial[g.id] = g.obligatorio && !g.multiple && g.opciones.length > 0 ? [g.opciones[0].id] : [];
    });
    return inicial;
  });
  const [cantidad, setCantidad] = useState(1);
  const [nota, setNota] = useState('');
  const [enviando, setEnviando] = useState(false);

  const alternar = (grupo, opcionId) => {
    setElegidas((actual) => {
      const lista = actual[grupo.id] || [];
      if (grupo.multiple) {
        return {
          ...actual,
          [grupo.id]: lista.includes(opcionId) ? lista.filter((id) => id !== opcionId) : [...lista, opcionId],
        };
      }
      // Una sola opción: tocar la elegida la quita (si no es obligatoria).
      if (lista.includes(opcionId) && !grupo.obligatorio) return { ...actual, [grupo.id]: [] };
      return { ...actual, [grupo.id]: [opcionId] };
    });
  };

  const extras = grupos.reduce(
    (s, g) =>
      s + g.opciones.filter((o) => (elegidas[g.id] || []).includes(o.id)).reduce((t, o) => t + o.precio_extra, 0),
    0
  );
  const precioUnitario = producto.precio + extras;
  const faltantes = grupos.filter((g) => g.obligatorio && (elegidas[g.id] || []).length === 0);

  const agregar = async () => {
    if (faltantes.length > 0 || enviando) return;
    setEnviando(true);
    await onAgregar({
      producto_id: producto.id,
      cantidad,
      opcion_ids: Object.values(elegidas).flat(),
      nota: nota.trim() || null,
    });
    setEnviando(false);
  };

  return (
    <div className="mesas-modal-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="mesas-modal opciones-modal">
        <div className="opciones-header">
          <h2>{producto.nombre}</h2>
          <span>S/ {producto.precio.toFixed(2)}</span>
        </div>

        <div className="opciones-cuerpo">
          {grupos.map((g) => (
            <div key={g.id} className="opciones-grupo">
              <div className="opciones-grupo-titulo">
                <strong>{g.nombre}</strong>
                <span className={g.obligatorio ? 'opciones-etiqueta-obligatoria' : 'opciones-etiqueta'}>
                  {g.obligatorio ? 'Obligatorio' : 'Opcional'}
                  {g.multiple ? ' · varias' : ''}
                </span>
              </div>
              <div className="opciones-lista">
                {g.opciones.map((o) => {
                  const activa = (elegidas[g.id] || []).includes(o.id);
                  return (
                    <button
                      key={o.id}
                      type="button"
                      className={`opciones-opcion${activa ? ' activa' : ''}`}
                      onClick={() => alternar(g, o.id)}
                    >
                      <span className={`opciones-marca${g.multiple ? ' cuadrada' : ''}`}>
                        {activa && <Check size={12} strokeWidth={3} />}
                      </span>
                      <span className="opciones-opcion-nombre">{o.nombre}</span>
                      {o.precio_extra > 0 && <span className="opciones-opcion-precio">+ S/ {o.precio_extra.toFixed(2)}</span>}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}

          <div className="opciones-grupo">
            <div className="opciones-grupo-titulo">
              <strong>Nota para barra/cocina</strong>
              <span className="opciones-etiqueta">Opcional</span>
            </div>
            <input
              className="mesas-input"
              value={nota}
              onChange={(e) => setNota(e.target.value)}
              placeholder="Ej. sin azúcar, bien caliente"
              maxLength={140}
            />
          </div>
        </div>

        <div className="opciones-pie">
          <div className="opciones-cantidad">
            <button type="button" onClick={() => setCantidad((c) => Math.max(1, c - 1))} aria-label="Menos">
              <Minus size={16} />
            </button>
            <span>{cantidad}</span>
            <button type="button" onClick={() => setCantidad((c) => Math.min(99, c + 1))} aria-label="Más">
              <Plus size={16} />
            </button>
          </div>
          <button type="button" className="mesas-boton-secundario" onClick={onCerrar}>
            Cancelar
          </button>
          <button
            type="button"
            className="mesas-boton-primario opciones-agregar"
            onClick={agregar}
            disabled={faltantes.length > 0 || enviando}
            title={faltantes.length ? `Falta elegir: ${faltantes.map((g) => g.nombre).join(', ')}` : undefined}
          >
            {enviando ? 'Agregando...' : `Agregar · S/ ${(precioUnitario * cantidad).toFixed(2)}`}
          </button>
        </div>
      </div>
    </div>
  );
}

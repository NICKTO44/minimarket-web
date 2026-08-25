import { useState, useEffect } from 'react';
import { api } from '../../api/api';
import './StockLotes.css';

const HORIZONTES = [
  { dias: 7, label: '7 días' },
  { dias: 15, label: '15 días' },
  { dias: 30, label: '30 días' },
];

export default function StockLotes() {
  const [horizonte, setHorizonte] = useState(15);
  const [lotes, setLotes] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [mensaje, setMensaje] = useState(null);

  const cargar = () => {
    setCargando(true);
    api
      .lotesPorVencer(horizonte)
      .then(setLotes)
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }))
      .finally(() => setCargando(false));
  };

  useEffect(() => {
    cargar();
  }, [horizonte]);

  const descartar = async (lote) => {
    const razon = lote.dias_restantes < 0 ? 'ya venció' : `vence en ${lote.dias_restantes} día(s)`;
    if (!confirm(`¿Descartar este lote de "${lote.producto_nombre}" (${razon})? Esto retira ${lote.cantidad} unidades del stock.`)) {
      return;
    }
    try {
      await api.loteDescartar(lote.lote_id);
      setMensaje({ tipo: 'exito', texto: 'Lote descartado y stock actualizado.' });
      cargar();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
  };

  const estadoDe = (dias) => {
    if (dias < 0) return 'vencido';
    if (dias <= 3) return 'critico';
    if (dias <= 7) return 'alerta';
    return 'normal';
  };

  const vencidos = lotes.filter((l) => l.dias_restantes < 0).length;

  return (
    <div className="sl-layout">
      <div className="sl-header">
        <h1>Stock y Lotes</h1>
        <div className="sl-horizontes">
          {HORIZONTES.map((h) => (
            <button
              key={h.dias}
              className={horizonte === h.dias ? 'activo' : ''}
              onClick={() => setHorizonte(h.dias)}
            >
              {h.label}
            </button>
          ))}
        </div>
      </div>

      <p className="sl-subtitulo">
        Lotes que vencen en los próximos {horizonte} días (o ya vencidos), ordenados del más urgente al menos urgente.
      </p>

      {vencidos > 0 && (
        <div className="sl-banner-vencidos">
          ⚠ Tienes {vencidos} lote(s) ya vencido(s). Revísalos y descártalos cuanto antes.
        </div>
      )}

      {mensaje && <p className={`sl-mensaje sl-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

      {cargando ? (
        <p className="sl-cargando">Cargando...</p>
      ) : lotes.length === 0 ? (
        <div className="sl-vacio">
          ✅ No hay lotes venciendo en los próximos {horizonte} días.
        </div>
      ) : (
        <div className="sl-lista">
          {lotes.map((l) => {
            const estado = estadoDe(l.dias_restantes);
            return (
              <div key={l.lote_id} className={`sl-item sl-item-${estado}`}>
                <div className="sl-item-info">
                  <span className="sl-item-nombre">{l.producto_nombre}</span>
                  <span className="sl-item-detalle">
                    {l.cantidad} {l.unidad_medida} — vence {l.fecha_vencimiento}
                  </span>
                </div>
                <div className="sl-item-derecha">
                  <span className={`sl-item-badge sl-item-badge-${estado}`}>
                    {l.dias_restantes < 0
                      ? `Vencido hace ${Math.abs(l.dias_restantes)}d`
                      : l.dias_restantes === 0
                      ? 'Vence hoy'
                      : `${l.dias_restantes}d restantes`}
                  </span>
                  <button className="sl-boton-descartar" onClick={() => descartar(l)}>
                    Descartar
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
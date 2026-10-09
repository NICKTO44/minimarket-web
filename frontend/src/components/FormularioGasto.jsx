import { useEffect, useRef, useState } from 'react';
import { ReceiptText, X } from 'lucide-react';
import { api } from '../api/api';
import { hoyLima } from '../utils/formato';
import { COMPROBANTES_GASTO, METODOS_GASTO } from '../utils/gastos';
import './FormularioGasto.css';

/**
 * Registrar un gasto del negocio. Lo usan la pantalla Gastos (todas las
 * formas de pago) y la Caja (soloCaja: sale del efectivo de la caja abierta,
 * con la fecha de hoy; así lo puede registrar también el cajero).
 */
export default function FormularioGasto({ soloCaja = false, efectivoCaja, onGuardado, onCerrar }) {
  const [categorias, setCategorias] = useState(null);
  // Efectivo que hay en la caja abierta (null = no hay caja abierta). La
  // Caja lo pasa ya calculado; desde Gastos se consulta.
  const [enCaja, setEnCaja] = useState(efectivoCaja);
  const [form, setForm] = useState({
    categoria_id: '',
    descripcion: '',
    monto: '',
    metodo_pago: soloCaja ? 'EFECTIVO_CAJA' : 'EFECTIVO',
    fecha: hoyLima(),
    pagado_a: '',
    comprobante_tipo: '',
    comprobante_numero: '',
  });
  const [error, setError] = useState('');
  const [guardando, setGuardando] = useState(false);
  const campoMonto = useRef(null);

  useEffect(() => {
    let vivo = true;
    api
      .gastoCategorias()
      .then((lista) => vivo && setCategorias(lista.filter((c) => c.activo)))
      .catch((e) => vivo && setError(e.message));
    if (efectivoCaja === undefined) {
      api
        .cajaAbierta()
        .then(
          (c) =>
            vivo &&
            setEnCaja(c ? c.monto_inicial + c.ventas_efectivo + c.ingresos_total - c.retiros_total - c.gastos_total : null)
        )
        .catch(() => vivo && setEnCaja(null));
    }
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => {
      vivo = false;
      window.removeEventListener('keydown', alTeclear);
    };
  }, [onCerrar, efectivoCaja]);

  const poner = (campo, valor) => setForm((f) => ({ ...f, [campo]: valor }));
  const deCaja = form.metodo_pago === 'EFECTIVO_CAJA';
  const monto = Number(String(form.monto).replace(',', '.'));
  const disponible = enCaja == null ? null : Math.max(0, Math.round(enCaja * 100) / 100);
  // Con efectivo de caja no se puede sacar más de lo que hay.
  const faltaEfectivo = deCaja && enCaja !== undefined && (disponible == null || monto > disponible + 0.005);
  const valido = form.categoria_id && form.descripcion.trim() && monto >= 0.01 && !faltaEfectivo;

  const guardar = async (e) => {
    e.preventDefault();
    if (!valido) return;
    setGuardando(true);
    setError('');
    try {
      const r = await api.gastoRegistrar({
        categoria_id: Number(form.categoria_id),
        descripcion: form.descripcion.trim(),
        monto,
        metodo_pago: form.metodo_pago,
        fecha: deCaja ? null : form.fecha,
        pagado_a: form.pagado_a.trim() || null,
        comprobante_tipo: form.comprobante_tipo || null,
        comprobante_numero: form.comprobante_tipo ? form.comprobante_numero.trim() || null : null,
      });
      onGuardado(r);
    } catch (err) {
      setError(err.message);
      setGuardando(false);
    }
  };

  return (
    <div className="fg-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <form className="fg-modal" onSubmit={guardar} role="dialog" aria-modal="true" aria-labelledby="fg-titulo">
        <header className="fg-cabecera">
          <span className="fg-icono">
            <ReceiptText size={18} />
          </span>
          <div>
            <h2 id="fg-titulo">Registrar gasto</h2>
            <p>{soloCaja ? 'Sale del efectivo de la caja abierta.' : 'Alquiler, servicios, sueldos, movilidad… (la mercadería va por Compras).'}</p>
          </div>
          <button type="button" className="fg-cerrar" onClick={onCerrar} aria-label="Cerrar">
            <X size={18} />
          </button>
        </header>

        <div className="fg-cuerpo">
          <div className="fg-campo">
            <span>Categoría</span>
            {!categorias && !error && <p className="fg-tenue">Cargando categorías…</p>}
            {categorias && (
              <div className="fg-chips" role="radiogroup" aria-label="Categoría">
                {categorias.map((c) => (
                  <button
                    type="button"
                    key={c.id}
                    role="radio"
                    aria-checked={String(c.id) === String(form.categoria_id)}
                    className={String(c.id) === String(form.categoria_id) ? 'activo' : ''}
                    onClick={() => {
                      poner('categoria_id', c.id);
                      campoMonto.current?.focus();
                    }}
                  >
                    {c.nombre}
                  </button>
                ))}
              </div>
            )}
          </div>

          <div className="fg-fila">
            <label className="fg-campo fg-monto">
              <span>Monto (S/)</span>
              <input
                ref={campoMonto}
                inputMode="decimal"
                placeholder="0.00"
                value={form.monto}
                onChange={(e) => poner('monto', e.target.value.replace(/[^\d.,]/g, ''))}
              />
            </label>
            <label className="fg-campo fg-ancho">
              <span>¿En qué se gastó?</span>
              <input
                value={form.descripcion}
                maxLength={200}
                placeholder="Ej. Recibo de luz de octubre"
                onChange={(e) => poner('descripcion', e.target.value)}
              />
            </label>
          </div>

          {!soloCaja && (
            <div className="fg-fila">
              <label className="fg-campo fg-ancho">
                <span>¿Cómo se pagó?</span>
                <select value={form.metodo_pago} onChange={(e) => poner('metodo_pago', e.target.value)}>
                  {METODOS_GASTO.map((m) => (
                    <option key={m.valor} value={m.valor}>
                      {m.etiqueta}
                    </option>
                  ))}
                </select>
              </label>
              <label className="fg-campo">
                <span>Fecha</span>
                <input
                  type="date"
                  value={deCaja ? hoyLima() : form.fecha}
                  max={hoyLima()}
                  disabled={deCaja}
                  onChange={(e) => poner('fecha', e.target.value)}
                />
              </label>
            </div>
          )}
          {deCaja && enCaja !== undefined && (
            <p className={`fg-nota${faltaEfectivo && monto >= 0.01 ? ' fg-nota-error' : ''}`}>
              {disponible == null
                ? 'No hay una caja abierta: elige otra forma de pago o abre la caja.'
                : faltaEfectivo && monto >= 0.01
                  ? `En la caja solo hay S/ ${disponible.toFixed(2)} en efectivo. Si lo pagaste con plata de otro lado o por Yape/Plin, ${soloCaja ? 'regístralo desde la pantalla Gastos' : 'elige esa forma de pago'}.`
                  : `En la caja hay S/ ${disponible.toFixed(2)} en efectivo. Se descuenta del efectivo esperado al cerrar; su fecha es la de hoy.`}
            </p>
          )}

          <label className="fg-campo">
            <span>Pagado a (opcional)</span>
            <input
              value={form.pagado_a}
              maxLength={120}
              placeholder="Ej. Electro Sur Este, Juan Pérez"
              onChange={(e) => poner('pagado_a', e.target.value)}
            />
          </label>

          <div className="fg-fila">
            <label className="fg-campo">
              <span>Comprobante (opcional)</span>
              <select value={form.comprobante_tipo} onChange={(e) => poner('comprobante_tipo', e.target.value)}>
                <option value="">Sin comprobante</option>
                {COMPROBANTES_GASTO.map((c) => (
                  <option key={c.valor} value={c.valor}>
                    {c.etiqueta}
                  </option>
                ))}
              </select>
            </label>
            {form.comprobante_tipo && (
              <label className="fg-campo fg-ancho">
                <span>Número</span>
                <input
                  value={form.comprobante_numero}
                  maxLength={40}
                  placeholder="Ej. F001-000123"
                  onChange={(e) => poner('comprobante_numero', e.target.value.toUpperCase())}
                />
              </label>
            )}
          </div>

          {error && <p className="fg-error">{error}</p>}
        </div>

        <footer className="fg-acciones">
          <button type="button" className="fg-boton" onClick={onCerrar}>
            Cancelar
          </button>
          <button type="submit" className="fg-boton fg-boton-principal" disabled={!valido || guardando}>
            {guardando ? 'Guardando…' : monto >= 0.01 ? `Registrar S/ ${monto.toFixed(2)}` : 'Registrar gasto'}
          </button>
        </footer>
      </form>
    </div>
  );
}

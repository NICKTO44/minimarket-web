import { useEffect, useState } from 'react';
import { ArrowDownToLine, ArrowUpFromLine, X } from 'lucide-react';
import { api } from '../api/api';
import { METODOS_RETIRO } from '../utils/gastos';
import './FormularioGasto.css';

const TIPOS = {
  RETIRO: {
    titulo: 'Retiro de efectivo',
    ayuda: 'Plata que se saca y no es un gasto (por ejemplo, el dueño se lleva lo vendido). Si pagaste algo (sueldo, pasaje, publicidad), regístralo como Gasto: un retiro no cuenta en el reporte de gastos.',
    ejemplo: 'Ej. Retiro del dueño a medio día',
    icono: ArrowUpFromLine,
  },
  INGRESO: {
    titulo: 'Ingreso de efectivo',
    ayuda: 'Plata que entra a la caja y no es una venta (por ejemplo, sencillo para dar vuelto).',
    ejemplo: 'Ej. Sencillo para vuelto',
    icono: ArrowDownToLine,
  },
};

/** Retiro o ingreso de efectivo en la caja abierta (los gastos van por FormularioGasto). */
export default function MovimientoCaja({ tipo, cajaId, usuario, disponible, onGuardado, onCerrar }) {
  const [monto, setMonto] = useState('');
  const [motivo, setMotivo] = useState('');
  // Solo el retiro elige con qué se hizo; lo que no es efectivo de la caja
  // queda en el turno sin descontar el efectivo esperado.
  const [metodo, setMetodo] = useState('EFECTIVO_CAJA');
  const deCaja = metodo === 'EFECTIVO_CAJA';
  const [error, setError] = useState('');
  const [guardando, setGuardando] = useState(false);
  const t = TIPOS[tipo];
  const Icono = t.icono;
  const valor = Number(String(monto).replace(',', '.'));
  // Un retiro no puede sacar más efectivo del que hay en la caja.
  const hay = Math.max(0, Math.round((disponible ?? 0) * 100) / 100);
  const excede = tipo === 'RETIRO' && deCaja && valor > hay + 0.005;

  useEffect(() => {
    const alTeclear = (e) => e.key === 'Escape' && onCerrar();
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar]);

  const guardar = async (e) => {
    e.preventDefault();
    if (!(valor >= 0.01) || !motivo.trim() || excede) return;
    setGuardando(true);
    setError('');
    try {
      await api.cajaMovimiento({
        caja_id: cajaId,
        tipo,
        monto: Math.round(valor * 100) / 100,
        motivo: motivo.trim(),
        usuario_id: usuario.id,
        metodo_pago: tipo === 'RETIRO' && !deCaja ? metodo : null,
      });
      onGuardado(deCaja);
    } catch (err) {
      setError(err.message);
      setGuardando(false);
    }
  };

  return (
    <div className="fg-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <form className="fg-modal" onSubmit={guardar} role="dialog" aria-modal="true" aria-labelledby="mc-titulo">
        <header className="fg-cabecera">
          <span className="fg-icono">
            <Icono size={18} />
          </span>
          <div>
            <h2 id="mc-titulo">{tipo === 'RETIRO' && !deCaja ? 'Retiro' : t.titulo}</h2>
            <p>{t.ayuda}</p>
          </div>
          <button type="button" className="fg-cerrar" onClick={onCerrar} aria-label="Cerrar">
            <X size={18} />
          </button>
        </header>
        <div className="fg-cuerpo">
          <div className="fg-fila">
            <label className="fg-campo fg-monto">
              <span>Monto (S/)</span>
              <input autoFocus inputMode="decimal" placeholder="0.00" value={monto} onChange={(e) => setMonto(e.target.value.replace(/[^\d.,]/g, ''))} />
            </label>
            <label className="fg-campo fg-ancho">
              <span>Motivo</span>
              <input value={motivo} maxLength={200} placeholder={t.ejemplo} onChange={(e) => setMotivo(e.target.value)} />
            </label>
          </div>
          {tipo === 'RETIRO' && (
            <label className="fg-campo">
              <span>¿Cómo se retiró?</span>
              <select value={metodo} onChange={(e) => setMetodo(e.target.value)}>
                {METODOS_RETIRO.map((m) => (
                  <option key={m.valor} value={m.valor}>
                    {m.etiqueta}
                  </option>
                ))}
              </select>
            </label>
          )}
          {tipo === 'RETIRO' && (
            <p className={`fg-nota${excede ? ' fg-nota-error' : ''}`}>
              {!deCaja
                ? 'Queda en los movimientos del turno. No descuenta el efectivo de la caja.'
                : excede
                  ? `No alcanza: en la caja solo hay S/ ${hay.toFixed(2)} en efectivo.`
                  : `En la caja hay S/ ${hay.toFixed(2)} en efectivo.`}
            </p>
          )}
          {error && <p className="fg-error">{error}</p>}
        </div>
        <footer className="fg-acciones">
          <button type="button" className="fg-boton" onClick={onCerrar}>
            Cancelar
          </button>
          <button type="submit" className="fg-boton fg-boton-principal" disabled={!(valor >= 0.01) || !motivo.trim() || excede || guardando}>
            {guardando ? 'Guardando…' : 'Registrar'}
          </button>
        </footer>
      </form>
    </div>
  );
}

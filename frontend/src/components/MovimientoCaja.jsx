import { useEffect, useState } from 'react';
import { ArrowDownToLine, ArrowUpFromLine, X } from 'lucide-react';
import { api } from '../api/api';
import './FormularioGasto.css';

const TIPOS = {
  RETIRO: {
    titulo: 'Retiro de efectivo',
    ayuda: 'Plata que se saca de la caja y no es un gasto (por ejemplo, el dueño se lleva lo vendido).',
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
  const [error, setError] = useState('');
  const [guardando, setGuardando] = useState(false);
  const t = TIPOS[tipo];
  const Icono = t.icono;
  const valor = Number(String(monto).replace(',', '.'));
  // Un retiro no puede sacar más efectivo del que hay en la caja.
  const hay = Math.max(0, Math.round((disponible ?? 0) * 100) / 100);
  const excede = tipo === 'RETIRO' && valor > hay + 0.005;

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
      await api.cajaMovimiento({ caja_id: cajaId, tipo, monto: Math.round(valor * 100) / 100, motivo: motivo.trim(), usuario_id: usuario.id });
      onGuardado();
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
            <h2 id="mc-titulo">{t.titulo}</h2>
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
            <p className={`fg-nota${excede ? ' fg-nota-error' : ''}`}>
              {excede ? `No alcanza: en la caja solo hay S/ ${hay.toFixed(2)} en efectivo.` : `En la caja hay S/ ${hay.toFixed(2)} en efectivo.`}
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

import { useState } from 'react';
import { X } from 'lucide-react';
import './AvisoVencimiento.css';

// Aviso sutil de que el plan mensual está por vencer (desde 5 días antes):
// una tarjeta pequeña flotando en la esquina, que no mueve la pantalla.
// Solo lo ve el administrador. Se puede cerrar: vuelve al día siguiente.
export const DIAS_AVISO_VENCIMIENTO = 5;

const hoyLocal = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
};

function fechaLarga(fecha) {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(fecha || '');
  if (!m) return null;
  return new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])).toLocaleDateString('es-PE', { weekday: 'long', day: 'numeric', month: 'long' });
}

export default function AvisoVencimiento({ dias, fechaVencimiento, negocio, onRenovar }) {
  const clave = `monspeet_aviso_vencimiento_${negocio || 'negocio'}`;
  const [cerradoHoy, setCerradoHoy] = useState(() => {
    try {
      return localStorage.getItem(clave) === hoyLocal();
    } catch {
      return false;
    }
  });

  if (dias == null || dias < 0 || dias > DIAS_AVISO_VENCIMIENTO || cerradoHoy) return null;

  const cerrar = () => {
    try {
      localStorage.setItem(clave, hoyLocal());
    } catch {
      // sin almacenamiento: solo se oculta por ahora
    }
    setCerradoHoy(true);
  };

  const fecha = fechaLarga(fechaVencimiento);
  const titulo = dias === 0 ? 'Tu plan vence hoy' : dias === 1 ? 'Tu plan vence mañana' : `Tu plan vence en ${dias} días`;

  return (
    <aside className={`aviso-venc ${dias <= 1 ? 'aviso-venc-pronto' : ''}`} role="status" aria-live="polite">
      <span className="aviso-venc-punto" aria-hidden="true" />
      <div className="aviso-venc-cuerpo">
        <p className="aviso-venc-titulo">{titulo}</p>
        <p className="aviso-venc-texto">
          {fecha && dias > 0 ? <>El {fecha}. </> : null}Renueva para seguir vendiendo sin pausas.
        </p>
        {onRenovar && (
          <button type="button" className="aviso-venc-renovar" onClick={onRenovar}>
            Renovar plan
          </button>
        )}
      </div>
      <button type="button" className="aviso-venc-cerrar" onClick={cerrar} aria-label="Ocultar el aviso hasta mañana" title="Ocultar hasta mañana">
        <X size={14} />
      </button>
    </aside>
  );
}

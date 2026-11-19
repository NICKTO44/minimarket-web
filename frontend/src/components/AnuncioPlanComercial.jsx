import { useState } from 'react';
import { X } from 'lucide-react';
import './AnuncioPlanComercial.css';

// Cada cuántos días vuelve a salir después de cerrarlo.
const DIAS_PARA_VOLVER = 7;
const WHATSAPP_NUMERO = '51910372220';

/**
 * Anuncio pequeño y discreto para negocios del plan Básico: los avisos de
 * Yape y Plin vienen desde el plan Comercial. Solo lo ve el administrador,
 * nunca en el punto de venta. Se cierra con la ✕ y vuelve a la semana.
 * Si está el aviso de vencimiento del plan, no sale (va primero ese).
 */
export default function AnuncioPlanComercial({ negocio, nombreNegocio }) {
  const clave = `monspeet_anuncio_comercial_${negocio || 'negocio'}`;
  const [oculto, setOculto] = useState(() => {
    try {
      const cerrado = Number(localStorage.getItem(clave) || 0);
      return cerrado > 0 && Date.now() - cerrado < DIAS_PARA_VOLVER * 24 * 60 * 60 * 1000;
    } catch {
      return false;
    }
  });

  if (oculto) return null;

  const cerrar = () => {
    try {
      localStorage.setItem(clave, String(Date.now()));
    } catch {
      // sin almacenamiento: solo se oculta por ahora
    }
    setOculto(true);
  };

  const texto = `Hola, quiero pasar al plan Comercial para tener los avisos de Yape y Plin${
    nombreNegocio ? ` en ${nombreNegocio}` : ''
  }.`;
  const enlace = `https://wa.me/${WHATSAPP_NUMERO}?text=${encodeURIComponent(texto)}`;

  return (
    <aside className="anuncio-plan" role="note">
      <span className="anuncio-plan-punto" aria-hidden="true" />
      <div className="anuncio-plan-cuerpo">
        <p className="anuncio-plan-titulo">Verifica cada Yape al instante</p>
        <p className="anuncio-plan-texto">
          Cada Yape o Plin que llega suena en tu tienda con su código, aunque el celular esté lejos. Adiós capturas
          falsas. Incluido en el plan Comercial.
        </p>
        <a className="anuncio-plan-boton" href={enlace} target="_blank" rel="noopener noreferrer" onClick={cerrar}>
          Quiero el plan Comercial
        </a>
      </div>
      <button type="button" className="anuncio-plan-cerrar" onClick={cerrar} aria-label="Ocultar el anuncio" title="Ocultar">
        <X size={14} />
      </button>
    </aside>
  );
}

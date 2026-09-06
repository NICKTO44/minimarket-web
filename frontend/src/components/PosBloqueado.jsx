import './PosBloqueado.css';

// Mismo número que en Login.jsx — si lo cambias ahí, cámbialo aquí también.
const WHATSAPP_NUMERO = '51999999999';
const WHATSAPP_MENSAJE = 'Hola, mi POS está bloqueado por suscripción vencida, quisiera regularizarlo.';
const ENLACE_WHATSAPP = `https://wa.me/${WHATSAPP_NUMERO}?text=${encodeURIComponent(WHATSAPP_MENSAJE)}`;

export default function PosBloqueado({ aviso }) {
  return (
    <div className="pos-bloqueado-container">
      <div className="pos-bloqueado-card">
        <div className="pos-bloqueado-icono">🔒</div>
        <h1>POS bloqueado</h1>
        <p className="pos-bloqueado-mensaje">
          {aviso || 'Tu suscripción no está al día. No puedes procesar ventas hasta regularizarla.'}
        </p>
        <a className="pos-bloqueado-whatsapp" href={ENLACE_WHATSAPP} target="_blank" rel="noopener noreferrer">
          Escríbenos por WhatsApp para renovar
        </a>
        <p className="pos-bloqueado-nota">
          El resto del sistema (productos, reportes, historial) sigue disponible para consulta.
        </p>
      </div>
    </div>
  );
}
import { useState } from 'react';
import { api } from '../../api/api';
import './Login.css';

// TODO: reemplazar por el número real de Monspeet Dev. (formato: código de país + número, sin +, sin espacios)
const WHATSAPP_NUMERO = '51999999999';
const WHATSAPP_MENSAJE = 'Hola, estoy interesado en el sistema, quisiera más información.';
const ENLACE_WHATSAPP = `https://wa.me/${WHATSAPP_NUMERO}?text=${encodeURIComponent(WHATSAPP_MENSAJE)}`;

function IconoWhatsapp() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
      <path d="M17.47 14.38c-.28-.14-1.67-.82-1.93-.92-.26-.1-.45-.14-.64.14-.19.28-.74.92-.9 1.1-.17.19-.33.21-.61.07-.28-.14-1.18-.44-2.24-1.39-.83-.74-1.39-1.65-1.55-1.93-.16-.28-.02-.43.12-.57.13-.13.28-.33.42-.5.14-.16.19-.28.28-.47.09-.19.05-.35-.02-.5-.07-.14-.64-1.56-.88-2.13-.23-.56-.47-.48-.64-.49-.16-.01-.35-.01-.54-.01-.19 0-.5.07-.76.35-.26.28-1 .98-1 2.4 0 1.42 1.03 2.78 1.17 2.98.14.19 2.03 3.1 4.93 4.34.69.3 1.22.48 1.64.61.69.22 1.32.19 1.81.11.55-.08 1.67-.68 1.91-1.34.24-.66.24-1.22.17-1.34-.07-.12-.26-.19-.54-.33z" />
      <path d="M12.02 2C6.5 2 2 6.48 2 11.98c0 1.87.51 3.62 1.4 5.13L2 22l5.02-1.32c1.46.8 3.13 1.25 4.9 1.25h.01c5.52 0 10.02-4.48 10.02-9.98C21.95 6.48 17.5 2 12.02 2zm0 18.14h-.01c-1.6 0-3.16-.43-4.52-1.24l-.32-.19-3.1.81.83-3.02-.21-.31A8.14 8.14 0 0 1 3.85 12c0-4.5 3.67-8.16 8.17-8.16 4.5 0 8.14 3.66 8.14 8.16 0 4.5-3.65 8.14-8.14 8.14z" />
    </svg>
  );
}

export default function Login({ tiendaRecordada, onLoginExitoso, onIrARegistro, onOlvidarTienda }) {
  const [usuario, setUsuario] = useState('');
  const [password, setPassword] = useState('');
  const [cargando, setCargando] = useState(false);
  const [mensaje, setMensaje] = useState('');

  const handleSubmit = async (e) => {
    e.preventDefault();
    setCargando(true);
    setMensaje('');
    try {
      const data = await api.login(usuario, password, tiendaRecordada?.identificador);
      if (data?.ok && data.token) {
        onLoginExitoso(data);
      }
    } catch (err) {
      setMensaje(err.message || 'Usuario o contraseña incorrectos');
    } finally {
      setCargando(false);
    }
  };

  // --- Login de un negocio ya identificado ---
  if (tiendaRecordada) {
    const inicial = (tiendaRecordada.nombre_negocio || '?').trim().charAt(0).toUpperCase();

    return (
      <div className="login-tienda-container">
        <form className="login-tienda-form" onSubmit={handleSubmit}>
          <div className="login-tienda-badge">{inicial}</div>
          <h1 className="login-tienda-nombre">{tiendaRecordada.nombre_negocio}</h1>
          <p className="login-tienda-subtitulo">Inicia sesión para continuar</p>

          <input
            type="text"
            placeholder="Usuario"
            value={usuario}
            onChange={(e) => setUsuario(e.target.value)}
            autoFocus
          />
          <input
            type="password"
            placeholder="Contraseña"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          <button type="submit" className="login-tienda-submit" disabled={cargando}>
            {cargando ? 'Ingresando...' : 'Ingresar'}
          </button>
          {mensaje && <p className="login-mensaje-error">{mensaje}</p>}

          <button type="button" className="login-tienda-cambiar" onClick={onOlvidarTienda}>
            ¿No es tu negocio? Cambiar
          </button>
        </form>
      </div>
    );
  }

  // --- Login general de la plataforma (sin negocio identificado todavía) ---
  return (
    <div className="login-general-container">
      <aside className="login-general-marca">
        <svg className="login-general-decoracion" viewBox="0 0 260 340" fill="none" aria-hidden="true">
          <path
            d="M30 10h150v270l-15-12-15 12-15-12-15 12-15-12-15 12-15-12-15 12-15-12-15 12V10z"
            stroke="currentColor"
            strokeWidth="2"
          />
          <line x1="55" y1="55" x2="155" y2="55" stroke="currentColor" strokeWidth="2" />
          <line x1="55" y1="80" x2="155" y2="80" stroke="currentColor" strokeWidth="2" />
          <line x1="55" y1="105" x2="120" y2="105" stroke="currentColor" strokeWidth="2" />
        </svg>

        <div className="login-general-marca-top">
          <p className="login-general-wordmark">
            Monspeet<span>POS</span>
          </p>
          <p className="login-general-tagline">
            Ventas, inventario y boletas electrónicas, todo en un solo sistema.
          </p>
        </div>

        <div className="login-general-marca-bottom">
          <a
            className="login-whatsapp-boton"
            href={ENLACE_WHATSAPP}
            target="_blank"
            rel="noopener noreferrer"
          >
            <IconoWhatsapp />
            Escríbenos por WhatsApp
          </a>
          <p className="login-general-firma">un producto de Monspeet Dev.</p>
        </div>
      </aside>

      <div className="login-general-panel">
        <form className="login-general-form" onSubmit={handleSubmit}>
          <h1>Ingresa a tu negocio</h1>

          <input
            type="text"
            placeholder="Usuario"
            value={usuario}
            onChange={(e) => setUsuario(e.target.value)}
            autoFocus
          />
          <input
            type="password"
            placeholder="Contraseña"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          <button type="submit" disabled={cargando}>
            {cargando ? 'Ingresando...' : 'Ingresar'}
          </button>
          {mensaje && <p className="login-mensaje-error">{mensaje}</p>}

          <button type="button" className="login-enlace" onClick={onIrARegistro}>
            ¿Tu negocio no está registrado todavía? Regístrate
          </button>
        </form>
      </div>
    </div>
  );
}

import { useState, useRef } from 'react';
import { api, API_URL } from '../../api/api';
import './Login.css';

// Número de WhatsApp de Monspeet Dev.
// Formato: código de país + número, sin +, sin espacios
const WHATSAPP_NUMERO = '51910372220';

const WHATSAPP_MENSAJE =
  'Hola, estoy interesado en el sistema, quisiera más información.';

const ENLACE_WHATSAPP = `https://wa.me/${WHATSAPP_NUMERO}?text=${encodeURIComponent(
  WHATSAPP_MENSAJE
)}`;

// ============================================================
// ICONO WHATSAPP
// ============================================================
function IconoWhatsapp() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="18"
      height="18"
      fill="currentColor"
      aria-hidden="true"
    >
      <path d="M17.47 14.38c-.28-.14-1.67-.82-1.93-.92-.26-.1-.45-.14-.64.14-.19.28-.74.92-.9 1.1-.17.19-.33.21-.61.07-.28-.14-1.18-.44-2.24-1.39-.83-.74-1.39-1.65-1.55-1.93-.16-.28-.02-.43.12-.57.13-.13.28-.33.42-.5.14-.16.19-.28.28-.47.09-.19.05-.35-.02-.5-.07-.14-.64-1.56-.88-2.13-.23-.56-.47-.48-.64-.49-.16-.01-.35-.01-.54-.01-.19 0-.5.07-.76.35-.26.28-1 .98-1 2.4 0 1.42 1.03 2.78 1.17 2.98.14.19 2.03 3.1 4.93 4.34.69.3 1.22.48 1.64.61.69.22 1.32.19 1.81.11.55-.08 1.67-.68 1.91-1.34.24-.66.24-1.22.17-1.34-.07-.12-.26-.19-.54-.33z" />
      <path d="M12.02 2C6.5 2 2 6.48 2 11.98c0 1.87.51 3.62 1.4 5.13L2 22l5.02-1.32c1.46.8 3.13 1.25 4.9 1.25h.01c5.52 0 10.02-4.48 10.02-9.98C21.95 6.48 17.5 2 12.02 2zm0 18.14h-.01c-1.6 0-3.16-.43-4.52-1.24l-.32-.19-3.1.81.83-3.02-.21-.31A8.14 8.14 0 0 1 3.85 12c0-4.5 3.67-8.16 8.17-8.16 4.5 0 8.14 3.66 8.14 8.16 0 4.5-3.65 8.14-8.14 8.14z" />
    </svg>
  );
}

function IconoCarrito() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      aria-hidden="true"
    >
      <circle cx="9" cy="20" r="1.4" />
      <circle cx="18" cy="20" r="1.4" />
      <path
        d="M2.5 3h2l2.4 12.3a2 2 0 0 0 2 1.7h8.2a2 2 0 0 0 2-1.6L21 8H6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function IconoRayo() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      aria-hidden="true"
    >
      <path
        d="M13 2 4 14h6l-1 8 9-12h-6l1-8Z"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
function IconoArrastre() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor" aria-hidden="true">
      <circle cx="8" cy="6" r="1.5" />
      <circle cx="16" cy="6" r="1.5" />
      <circle cx="8" cy="12" r="1.5" />
      <circle cx="16" cy="12" r="1.5" />
      <circle cx="8" cy="18" r="1.5" />
      <circle cx="16" cy="18" r="1.5" />
    </svg>
  );
}

function IconoCaja() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      aria-hidden="true"
    >
      <path
        d="M3.5 7.5 12 3l8.5 4.5v9L12 21l-8.5-4.5v-9Z"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M3.5 7.5 12 12l8.5-4.5M12 12v9"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function IconoDocumento() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      aria-hidden="true"
    >
      <path
        d="M6 2.5h8l4 4V21a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V3.5a1 1 0 0 1 1-1Z"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path d="M9 13h6M9 17h6" strokeLinecap="round" />
    </svg>
  );
}

// ============================================================
// OJO ABIERTO: CONTRASEÑA VISIBLE
// ============================================================
function IconoOjo() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="18"
      height="18"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      aria-hidden="true"
    >
      <path
        d="M1.5 12S5 5 12 5s10.5 7 10.5 7-3.5 7-10.5 7S1.5 12 1.5 12z"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle
        cx="12"
        cy="12"
        r="3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

// ============================================================
// OJO TACHADO: CONTRASEÑA OCULTA
// ============================================================
function IconoOjoTachado() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="18"
      height="18"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      aria-hidden="true"
    >
      <path d="M3 3l18 18" strokeLinecap="round" />

      <path
        d="M10.6 5.2A10.6 10.6 0 0 1 12 5c7 0 10.5 7 10.5 7a13.3 13.3 0 0 1-3.1 4.1M6.6 6.6C3.6 8.5 1.5 12 1.5 12S5 19 12 19c1.4 0 2.7-.28 3.9-.75"
        strokeLinecap="round"
        strokeLinejoin="round"
      />

      <path
        d="M9.9 9.9a3 3 0 0 0 4.2 4.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

// ============================================================
// COMPONENTE LOGIN
// ============================================================
export default function Login({
  tiendaRecordada,
  onLoginExitoso,
  onTiendaIdentificada,
  onIrARegistro,
  onOlvidarTienda,
}) {
  const [usuario, setUsuario] = useState('');
  const [password, setPassword] = useState('');
  const [cargando, setCargando] = useState(false);
  const [mensaje, setMensaje] = useState('');
  const [mostrarPassword, setMostrarPassword] = useState(false);
  // ============================================================
  // ARRASTRAR EL MODAL DE LOGIN (solo desktop -- en mobile el
  // CSS fuerza transform: none, así que esto no tiene efecto ahí)
  // ============================================================
  // ============================================================
  // ARRASTRAR EL MODAL DE LOGIN (solo desktop -- en mobile el
  // CSS fuerza transform: none, así que esto no tiene efecto ahí)
  // ============================================================
  const [posicion, setPosicion] = useState({ x: 0, y: 0 });
  const arrastrando = useRef(false);
  const offsetArrastre = useRef({ x: 0, y: 0 });

  const handleArrastreDown = (e) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    arrastrando.current = true;
    offsetArrastre.current = {
      x: e.clientX - posicion.x,
      y: e.clientY - posicion.y,
    };
  };

  const handleArrastreMove = (e) => {
    if (!arrastrando.current) return;
    setPosicion({
      x: e.clientX - offsetArrastre.current.x,
      y: e.clientY - offsetArrastre.current.y,
    });
  };

  const handleArrastreUp = (e) => {
    arrastrando.current = false;
    e.currentTarget.releasePointerCapture(e.pointerId);
  };
  // ============================================================
  // LOGIN DE UN NEGOCIO YA IDENTIFICADO (PASO 2)
  // ============================================================
  const handleSubmitLogin = async (e) => {
    e.preventDefault();

    setCargando(true);
    setMensaje('');

    try {
      const data = await api.login(
        usuario,
        password,
        tiendaRecordada?.identificador
      );

      if (data?.ok && data.token) {
        onLoginExitoso(data);
      }
    } catch (err) {
      setMensaje(err.message || 'Usuario o contraseña incorrectos');
    } finally {
      setCargando(false);
    }
  };

  // ============================================================
  // PASO 1: IDENTIFICAR EL NEGOCIO POR USUARIO
  // ============================================================
  const handleSubmitIdentificar = async (e) => {
    e.preventDefault();

    setCargando(true);
    setMensaje('');

    try {
      const data = await api.identificarUsuario(usuario);

      if (data?.ok && data.tienda) {
        onTiendaIdentificada(data.tienda);
      }
    } catch (err) {
      setMensaje(err.message || 'No se pudo identificar el negocio');
    } finally {
      setCargando(false);
    }
  };

  // ============================================================
  // LOGIN DE UN NEGOCIO YA IDENTIFICADO
  // ============================================================
  if (tiendaRecordada) {
    const inicial = (tiendaRecordada.nombre_negocio || '?')
      .trim()
      .charAt(0)
      .toUpperCase();

    const colorAcento = tiendaRecordada.color_acento || '#4338ca';

    return (
      <div className="login-tienda-container">
        <form
          className="login-tienda-form"
          onSubmit={handleSubmitLogin}
        >
          {tiendaRecordada.logo_url ? (
            <img
              src={`${API_URL}${tiendaRecordada.logo_url}?t=${Date.now()}`}
              alt={tiendaRecordada.nombre_negocio}
              className="login-tienda-logo"
            />
          ) : (
            <div
              className="login-tienda-badge"
              style={{
                background: colorAcento,
                color: '#fff',
              }}
            >
              {inicial}
            </div>
          )}

          <h1 className="login-tienda-nombre">
            {tiendaRecordada.nombre_negocio}
          </h1>

          <p className="login-tienda-subtitulo">
            Inicia sesión para continuar
          </p>

          <input
            type="text"
            placeholder="Usuario"
            value={usuario}
            onChange={(e) => setUsuario(e.target.value)}
            autoFocus
          />

          <div className="login-campo-password">
            <input
              type={mostrarPassword ? 'text' : 'password'}
              placeholder="Contraseña"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              style={{
                '--color-foco': colorAcento,
              }}
            />

            <button
              type="button"
              className="login-toggle-password"
              onClick={() =>
                setMostrarPassword((v) => !v)
              }
              aria-label={
                mostrarPassword
                  ? 'Ocultar contraseña'
                  : 'Mostrar contraseña'
              }
              tabIndex={-1}
            >
              {mostrarPassword ? (
                <IconoOjoTachado />
              ) : (
                <IconoOjo />
              )}
            </button>
          </div>

          <button
            type="submit"
            className="login-tienda-submit"
            disabled={cargando}
            style={{
              background: colorAcento,
            }}
          >
            {cargando ? 'Ingresando...' : 'Ingresar'}
          </button>

          {mensaje && (
            <p className="login-mensaje-error">
              {mensaje}
            </p>
          )}

          <button
            type="button"
            className="login-tienda-cambiar"
            onClick={onOlvidarTienda}
            style={{
              '--color-hover': colorAcento,
            }}
          >
            ¿No es tu negocio? Cambiar
          </button>
        </form>
      </div>
    );
  }

  // ============================================================
  // LOGIN GENERAL DE LA PLATAFORMA
  // PASO 1: IDENTIFICAR EL NEGOCIO
  // Imagen a pantalla completa, marca a la izquierda, login como
  // tarjeta flotante superpuesta sobre la imagen.
  // ============================================================
  return (
    <div className="login-general-container">
      <div className="login-general-marca-contenido">
        <div className="login-general-logo">
          <IconoCarrito />

          <p className="login-general-wordmark">
            Monspeet<span>POS</span>
          </p>
        </div>

        <h2 className="login-general-titulo">
          Tu negocio, más simple,
          <br />
          <span>más rentable.</span>
        </h2>

        <p className="login-general-tagline">
          Sistema de punto de venta en la nube para tiendas, restaurantes
          y todo tipo de negocios.
        </p>

        <div className="login-general-features">
          <div className="login-general-feature">
            <span className="login-general-feature-icono">
              <IconoRayo />
            </span>

            <p>Ventas rápidas y simples</p>
          </div>

          <div className="login-general-feature">
            <span className="login-general-feature-icono">
              <IconoCaja />
            </span>

            <p>Control de inventario en tiempo real</p>
          </div>

          <div className="login-general-feature">
            <span className="login-general-feature-icono">
              <IconoDocumento />
            </span>

            <p>Boletas y facturas electrónicas SUNAT</p>
          </div>
        </div>

        <a
          className="login-whatsapp-boton"
          href={ENLACE_WHATSAPP}
          target="_blank"
          rel="noopener noreferrer"
          aria-label="Escríbenos por WhatsApp"
        >
          <IconoWhatsapp />
          Escríbenos por WhatsApp
        </a>

        <p className="login-general-firma">
          un producto de Monspeet Dev.
        </p>
      </div>
      <div
        className="login-general-flotante"
        style={{
          transform: `translate(calc(-50% + ${posicion.x}px), calc(-50% + ${posicion.y}px))`,
        }}
      >
        <div
          className="login-general-arrastre"
          onPointerDown={handleArrastreDown}
          onPointerMove={handleArrastreMove}
          onPointerUp={handleArrastreUp}
          aria-hidden="true"
        >
          <IconoArrastre />
        </div>

        <form
          className="login-general-form"
          onSubmit={handleSubmitIdentificar}
        >
          <h1>
            Ingresa a tu negocio
          </h1>

          <input
            type="text"
            placeholder="Usuario"
            value={usuario}
            onChange={(e) => setUsuario(e.target.value)}
            autoFocus
          />

          <button
            type="submit"
            disabled={cargando}
          >
            {cargando ? 'Buscando...' : 'Continuar'}
          </button>

          {mensaje && (
            <p className="login-mensaje-error">
              {mensaje}
            </p>
          )}

          <button
            type="button"
            className="login-enlace"
            onClick={onIrARegistro}
          >
            ¿Tu negocio no está registrado todavía?
            Regístrate
          </button>
        </form>
      </div>
    </div>
  );
}


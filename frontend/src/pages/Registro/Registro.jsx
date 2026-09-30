import { useState, useEffect } from 'react';
import { Coffee, Store } from 'lucide-react';
import { api } from '../../api/api';
import '../Login/Login.css';
import './Registro.css';

const DEBOUNCE_MS = 500;

// Tipo de negocio: decide qué módulos se activan. Se puede cambiar
// después en Configuración, así que no es una decisión para siempre.
const TIPOS_NEGOCIO = [
  {
    valor: 'TIENDA',
    titulo: 'Tienda',
    detalle: 'Bodega, minimarket, ferretería, farmacia...',
    Icono: Store,
  },
  {
    valor: 'RESTAURANTE',
    titulo: 'Cafetería / Restaurante',
    detalle: 'Atención en mesas, pedidos y comandas',
    Icono: Coffee,
  },
];

export default function Registro({ onRegistroExitoso, onIrALogin }) {
  const [nombreNegocio, setNombreNegocio] = useState('');
  const [nombreCompleto, setNombreCompleto] = useState('');
  const [usuario, setUsuario] = useState('');
  const [password, setPassword] = useState('');
  const [ruc, setRuc] = useState('');
  const [modoNegocio, setModoNegocio] = useState('TIENDA');

  const [disponible, setDisponible] = useState(null); // null = sin chequear todavía
  const [verificando, setVerificando] = useState(false);
  const [creando, setCreando] = useState(false);
  const [mensaje, setMensaje] = useState('');

  useEffect(() => {
    const texto = usuario.trim();
    if (texto.length < 3) {
      setDisponible(null);
      return;
    }
    setVerificando(true);
    const timeout = setTimeout(() => {
      api
        .verificarUsuario(texto)
        .then((r) => setDisponible(r.disponible))
        .catch(() => setDisponible(null))
        .finally(() => setVerificando(false));
    }, DEBOUNCE_MS);
    return () => clearTimeout(timeout);
  }, [usuario]);

  const puedeRegistrar =
    nombreNegocio.trim().length > 0 &&
    nombreCompleto.trim().length > 0 &&
    usuario.trim().length >= 3 &&
    disponible === true &&
    password.length >= 6 &&
    !creando;

  const handleSubmit = async (e) => {
    e.preventDefault();
    setMensaje('');
    if (!puedeRegistrar) return;

    setCreando(true);
    try {
      const registro = await api.registro({
        nombre_negocio: nombreNegocio.trim(),
        nombre_completo: nombreCompleto.trim(),
        usuario: usuario.trim(),
        password,
        ruc: ruc.trim() || null,
        modo_negocio: modoNegocio,
      });

      // El negocio ya existe con este usuario como súper admin — entra
      // directo, sin pedirle que haga login por separado.
      const login = await api.login(usuario.trim(), password, registro.identificador);
      if (login?.ok && login.token) {
        onRegistroExitoso(login);
      }
    } catch (err) {
      setMensaje(err.message || 'No se pudo completar el registro');
    } finally {
      setCreando(false);
    }
  };

  return (
    <div className="login-container registro-container">
      <form className="login-form registro-form" onSubmit={handleSubmit}>
        <h1>Registra tu negocio</h1>

        <div className="registro-tipos" role="radiogroup" aria-label="Tipo de negocio">
          {TIPOS_NEGOCIO.map(({ valor, titulo, detalle, Icono }) => (
            <button
              key={valor}
              type="button"
              role="radio"
              aria-checked={modoNegocio === valor}
              className={`registro-tipo${modoNegocio === valor ? ' activo' : ''}`}
              onClick={() => setModoNegocio(valor)}
            >
              <Icono size={22} strokeWidth={2} />
              <strong>{titulo}</strong>
              <span>{detalle}</span>
            </button>
          ))}
        </div>

        <input
          type="text"
          placeholder="Nombre del negocio"
          value={nombreNegocio}
          onChange={(e) => setNombreNegocio(e.target.value)}
          autoFocus
        />
        <input
          type="text"
          placeholder="Tu nombre completo"
          value={nombreCompleto}
          onChange={(e) => setNombreCompleto(e.target.value)}
        />

        <div className="registro-campo-usuario">
          <input
            type="text"
            placeholder="Elige un usuario"
            value={usuario}
            onChange={(e) => setUsuario(e.target.value)}
          />
          {usuario.trim().length >= 3 && (
            <span
              className={`registro-usuario-estado ${
                verificando ? 'verificando' : disponible ? 'disponible' : 'ocupado'
              }`}
            >
              {verificando ? 'Comprobando...' : disponible ? '✓ Disponible' : '✗ Ya está en uso'}
            </span>
          )}
        </div>

        <input
          type="password"
          placeholder="Contraseña (mínimo 6 caracteres)"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <input
          type="text"
          placeholder="RUC (opcional)"
          value={ruc}
          onChange={(e) => setRuc(e.target.value)}
        />

        <button type="submit" disabled={!puedeRegistrar}>
          {creando ? 'Creando tu negocio...' : 'Crear mi negocio'}
        </button>
        {mensaje && <p className="login-mensaje-error">{mensaje}</p>}

        <button type="button" className="login-enlace" onClick={onIrALogin}>
          ¿Ya tienes cuenta? Inicia sesión
        </button>
      </form>
    </div>
  );
}
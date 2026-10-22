import { useState, useEffect, useRef } from 'react';
import { api } from '../../api/api';
import { MODULO_MESAS, RUBROS } from '../../utils/rubros';
import {
  errorFormatoRuc,
  errorNombre,
  errorUsuario,
  datoReal,
  baseUsuarioSugerido,
  candidatosUsuario,
} from '../../utils/validacionRegistro';
import TarjetaRuc from '../../components/TarjetaRuc';
import { sugerirEn } from '../../utils/sugerencia';
import '../Login/Login.css';
import './Registro.css';

const DEBOUNCE_MS = 500;
const DEBOUNCE_RUC_MS = 300;

// El rubro decide con qué módulos, unidades y nombres de pantalla arranca
// el negocio (utils/rubros.js). Se puede cambiar después en Configuración,
// así que no es una decisión para siempre.

// "QUISPE MAMANI JUAN CARLOS" (como lo da SUNAT) -> "Juan Carlos Quispe Mamani".
function nombreDePersonaNatural(razonSocial) {
  const palabras = datoReal(razonSocial).toLowerCase().split(/\s+/).filter(Boolean);
  const capital = (p) => p.charAt(0).toUpperCase() + p.slice(1);
  const ordenadas = palabras.length >= 3 ? [...palabras.slice(2), ...palabras.slice(0, 2)] : palabras;
  return ordenadas.map(capital).join(' ');
}

export default function Registro({ onRegistroExitoso, onIrALogin }) {
  const [nombreNegocio, setNombreNegocio] = useState('');
  const [nombreCompleto, setNombreCompleto] = useState('');
  const [usuario, setUsuario] = useState('');
  const [password, setPassword] = useState('');
  const [ruc, setRuc] = useState('');
  const [rubro, setRubro] = useState(RUBROS[0].valor);

  const [disponible, setDisponible] = useState(null); // null = sin chequear todavía
  const [verificando, setVerificando] = useState(false);
  const [creando, setCreando] = useState(false);
  const [mensaje, setMensaje] = useState('');

  // Última respuesta de la consulta del RUC: { ruc, resultado, mensaje, datos }.
  const [respuestaRuc, setRespuestaRuc] = useState(null);
  // Campos que el usuario ya dejó (para no mostrar errores mientras recién escribe).
  const [tocados, setTocados] = useState({});
  const tocar = (campo) => () => setTocados((t) => ({ ...t, [campo]: true }));

  // Lo último que se sugirió en cada campo: si el usuario no lo cambió, una
  // nueva consulta de RUC puede reemplazarlo; si lo escribió él, se respeta.
  const sugerido = useRef({ negocio: '', nombre: '', usuario: '' });
  const consultaActual = useRef(0);

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

  // Sugiere un usuario libre a partir del RUC consultado.
  const sugerirUsuario = async (rucConsultado, datos, idConsulta) => {
    const base = baseUsuarioSugerido({
      ruc: rucConsultado,
      razonSocial: datos.razon_social,
      nombreComercial: datos.nombre_comercial,
    });
    for (const candidato of candidatosUsuario(base, rucConsultado)) {
      const r = await api.verificarUsuario(candidato).catch(() => ({ disponible: null }));
      if (consultaActual.current !== idConsulta) return; // ya cambiaron el RUC
      if (r.disponible === true) {
        sugerirEn(setUsuario, sugerido, 'usuario', candidato);
        return;
      }
      if (r.disponible === null) return; // sin conexión: no insistir
    }
  };

  // Al completar los 11 dígitos: formato primero (sin gastar consultas) y,
  // si está bien, los datos de SUNAT.
  useEffect(() => {
    const id = ++consultaActual.current;
    if (ruc.length !== 11 || errorFormatoRuc(ruc)) return;
    const timeout = setTimeout(async () => {
      const r = await api.consultarRucRegistro(ruc);
      if (consultaActual.current !== id) return;
      setRespuestaRuc({ ruc, ...r });
      if (r.resultado !== 'ENCONTRADO' || !r.datos) return;

      const datos = r.datos;
      const negocioSugerido = datoReal(datos.nombre_comercial) || datoReal(datos.razon_social);
      sugerirEn(setNombreNegocio, sugerido, 'negocio', negocioSugerido);
      if (ruc.startsWith('10')) {
        sugerirEn(setNombreCompleto, sugerido, 'nombre', nombreDePersonaNatural(datos.razon_social));
      }
      sugerirUsuario(ruc, datos, id);
    }, DEBOUNCE_RUC_MS);
    return () => clearTimeout(timeout);
  }, [ruc]);

  // Estado de la consulta para el RUC que está escrito ahora mismo.
  const rucConFormato = ruc.length === 11 && !errorFormatoRuc(ruc);
  const consultaRuc = !rucConFormato
    ? { estado: 'inactivo' }
    : respuestaRuc?.ruc === ruc
      ? { estado: 'listo', ...respuestaRuc }
      : { estado: 'consultando' };

  const errorRuc = ruc.length === 11 || tocados.ruc ? errorFormatoRuc(ruc) : null;
  const rucNoExiste = consultaRuc.estado === 'listo' && consultaRuc.resultado === 'NO_EXISTE';
  const errores = {
    negocio: nombreNegocio.trim() ? errorNombre(nombreNegocio, 'El nombre del negocio') : null,
    nombre: nombreCompleto.trim() ? errorNombre(nombreCompleto, 'Tu nombre') : null,
    usuario: usuario.trim() ? errorUsuario(usuario) : null,
  };

  const puedeRegistrar =
    !errorFormatoRuc(ruc) &&
    consultaRuc.estado === 'listo' &&
    !rucNoExiste &&
    nombreNegocio.trim().length > 0 &&
    nombreCompleto.trim().length > 0 &&
    usuario.trim().length >= 3 &&
    !errores.negocio &&
    !errores.nombre &&
    !errores.usuario &&
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
        rubro,
        // Para un servidor que aún no conoce los rubros: lo de siempre.
        modo_negocio: RUBROS.find((r) => r.valor === rubro)?.modulos.includes(MODULO_MESAS) ? 'RESTAURANTE' : 'TIENDA',
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

  // Solo se muestra el error de un campo cuando ya lo dejó.
  const errorVisible = (campo) => (tocados[campo] ? errores[campo] : null);

  return (
    <div className="login-container registro-container">
      <form className="login-form registro-form" onSubmit={handleSubmit}>
        <h1>Registra tu negocio</h1>

        <div className="registro-campo">
          <label className="registro-etiqueta" htmlFor="registro-ruc">RUC</label>
          <input
            id="registro-ruc"
            type="text"
            inputMode="numeric"
            autoComplete="off"
            placeholder="RUC de tu negocio"
            maxLength={11}
            value={ruc}
            onChange={(e) => setRuc(e.target.value.replace(/\D/g, '').slice(0, 11))}
            onBlur={tocar('ruc')}
            aria-invalid={Boolean(errorRuc || rucNoExiste)}
            autoFocus
          />
          {errorRuc && <span className="registro-campo-error">{errorRuc}</span>}
          {!errorRuc && consultaRuc.estado === 'consultando' && (
            <span className="registro-campo-info">Consultando SUNAT...</span>
          )}
          {consultaRuc.estado === 'listo' && consultaRuc.resultado === 'NO_EXISTE' && (
            <span className="registro-campo-error">{consultaRuc.mensaje}</span>
          )}
          {consultaRuc.estado === 'listo' && consultaRuc.resultado === 'NO_DISPONIBLE' && (
            <span className="registro-campo-info">{consultaRuc.mensaje}</span>
          )}
          {consultaRuc.estado === 'listo' && consultaRuc.resultado === 'ENCONTRADO' && consultaRuc.datos && (
            <>
              <TarjetaRuc ruc={ruc} datos={consultaRuc.datos} />
              <span className="registro-campo-info">
                Te sugerimos estos datos según tu RUC. Puedes cambiarlos.
              </span>
            </>
          )}
        </div>

        <p className="registro-rubro-titulo">¿Qué rubro es tu negocio?</p>
        <div className="registro-tipos" role="radiogroup" aria-label="Rubro del negocio">
          {RUBROS.map(({ valor, label, Icono }) => (
            <button
              key={valor}
              type="button"
              role="radio"
              aria-checked={rubro === valor}
              className={`registro-tipo${rubro === valor ? ' activo' : ''}`}
              onClick={() => setRubro(valor)}
            >
              <Icono size={18} strokeWidth={2} />
              <strong>{label}</strong>
            </button>
          ))}
        </div>
        {/* La descripción solo del rubro elegido: la lista queda corta y el
            formulario no se va hacia abajo. */}
        <p className="registro-rubro-descripcion">{RUBROS.find((r) => r.valor === rubro)?.descripcion}</p>

        <div className="registro-campo">
          <label className="registro-etiqueta" htmlFor="registro-negocio">Nombre del negocio</label>
          <input
            id="registro-negocio"
            type="text"
            placeholder="Nombre del negocio"
            value={nombreNegocio}
            onChange={(e) => setNombreNegocio(e.target.value)}
            onBlur={tocar('negocio')}
            aria-invalid={Boolean(errorVisible('negocio'))}
          />
          {errorVisible('negocio') && <span className="registro-campo-error">{errorVisible('negocio')}</span>}
        </div>

        <div className="registro-campo">
          <label className="registro-etiqueta" htmlFor="registro-nombre">Tu nombre completo</label>
          <input
            id="registro-nombre"
            type="text"
            placeholder="Tu nombre completo"
            value={nombreCompleto}
            onChange={(e) => setNombreCompleto(e.target.value)}
            onBlur={tocar('nombre')}
            aria-invalid={Boolean(errorVisible('nombre'))}
          />
          {errorVisible('nombre') && <span className="registro-campo-error">{errorVisible('nombre')}</span>}
        </div>

        <div className="registro-campo-usuario">
          <label className="registro-etiqueta" htmlFor="registro-usuario">Usuario para iniciar sesión</label>
          <input
            id="registro-usuario"
            type="text"
            placeholder="Elige un usuario"
            value={usuario}
            onChange={(e) => setUsuario(e.target.value)}
            onBlur={tocar('usuario')}
            aria-invalid={Boolean(errorVisible('usuario'))}
          />
          {errorVisible('usuario') ? (
            <span className="registro-campo-error">{errorVisible('usuario')}</span>
          ) : (
            usuario.trim().length >= 3 && (
              <span
                className={`registro-usuario-estado ${
                  verificando ? 'verificando' : disponible ? 'disponible' : 'ocupado'
                }`}
              >
                {verificando ? 'Comprobando...' : disponible ? '✓ Disponible' : '✗ Ya está en uso'}
              </span>
            )
          )}
        </div>

        <div className="registro-campo">
          <label className="registro-etiqueta" htmlFor="registro-clave">Contraseña</label>
          <input
            id="registro-clave"
            type="password"
            placeholder="Mínimo 6 caracteres"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </div>

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

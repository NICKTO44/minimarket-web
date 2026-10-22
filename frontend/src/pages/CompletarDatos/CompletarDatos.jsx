import { useRef, useState } from 'react';
import { api } from '../../api/api';
import TarjetaRuc from '../../components/TarjetaRuc';
import { useConsultaRuc } from '../../utils/useConsultaRuc';
import { sugerirEn } from '../../utils/sugerencia';
import { datoReal, errorFormatoRuc, errorNombre } from '../../utils/validacionRegistro';
import '../Login/Login.css';
import '../Registro/Registro.css';

// Se muestra al administrador de un negocio cuyo RUC guardado no es válido
// (por ejemplo, uno creado antes de que el registro validara el RUC). No
// puede usar el sistema hasta corregirlo. Cambia solo nombre, RUC y
// dirección (PUT /configuracion/datos-negocio); el resto de la configuración
// no se toca. El backend vuelve a validar todo.
export default function CompletarDatos({ configuracion, onListo, onLogout }) {
  const nombreActual = configuracion?.nombre_tienda || '';
  const [ruc, setRuc] = useState('');
  const [nombre, setNombre] = useState(errorNombre(nombreActual, 'x') ? '' : nombreActual);
  const [direccion, setDireccion] = useState(configuracion?.direccion || '');
  const [tocados, setTocados] = useState({});
  const [guardando, setGuardando] = useState(false);
  const [mensaje, setMensaje] = useState('');
  // Lo último que se sugirió: si el usuario lo cambió, una nueva consulta lo
  // respeta. La dirección vieja cuenta como sugerida: la del RUC la reemplaza.
  const sugerido = useRef({ nombre: '', direccion: configuracion?.direccion || '' });

  const consulta = useConsultaRuc(ruc, true, (datos) => {
    sugerirEn(setNombre, sugerido, 'nombre', datoReal(datos.nombre_comercial) || datoReal(datos.razon_social));
    sugerirEn(setDireccion, sugerido, 'direccion', datoReal(datos.direccion));
  });

  const errorRuc = ruc.length === 11 || tocados.ruc ? errorFormatoRuc(ruc) : null;
  const rucNoExiste = consulta.estado === 'listo' && consulta.resultado === 'NO_EXISTE';
  const errorDeNombre = nombre.trim() ? errorNombre(nombre, 'El nombre del negocio') : null;

  const puedeGuardar =
    !errorFormatoRuc(ruc) &&
    consulta.estado === 'listo' &&
    !rucNoExiste &&
    nombre.trim().length > 0 &&
    !errorDeNombre &&
    !guardando;

  const guardar = async (e) => {
    e.preventDefault();
    if (!puedeGuardar) return;
    setMensaje('');
    setGuardando(true);
    try {
      await api.datosNegocioGuardar({
        nombre_tienda: nombre.trim(),
        ruc,
        direccion: direccion.trim() || null,
      });
      onListo(await api.configuracionObtener());
    } catch (err) {
      setMensaje(err.message || 'No se pudieron guardar los datos');
    } finally {
      setGuardando(false);
    }
  };

  return (
    <div className="login-container registro-container">
      <form className="login-form registro-form" onSubmit={guardar}>
        <h1>Completa los datos de tu negocio</h1>
        <p className="registro-campo-info">
          El RUC guardado{configuracion?.ruc ? ` («${configuracion.ruc}»)` : ''} no es válido. Para seguir usando
          Monspeet, ingresa el RUC de tu negocio: lo verificamos con SUNAT.
        </p>

        <div className="registro-campo">
          <label className="registro-etiqueta" htmlFor="completar-ruc">RUC</label>
          <input
            id="completar-ruc"
            type="text"
            inputMode="numeric"
            autoComplete="off"
            placeholder="RUC de tu negocio"
            maxLength={11}
            value={ruc}
            onChange={(e) => setRuc(e.target.value.replace(/\D/g, '').slice(0, 11))}
            onBlur={() => setTocados((t) => ({ ...t, ruc: true }))}
            aria-invalid={Boolean(errorRuc || rucNoExiste)}
            autoFocus
          />
          {errorRuc && <span className="registro-campo-error">{errorRuc}</span>}
          {!errorRuc && consulta.estado === 'consultando' && (
            <span className="registro-campo-info">Consultando SUNAT...</span>
          )}
          {rucNoExiste && <span className="registro-campo-error">{consulta.mensaje}</span>}
          {consulta.estado === 'listo' && consulta.resultado === 'NO_DISPONIBLE' && (
            <span className="registro-campo-info">{consulta.mensaje}</span>
          )}
          {consulta.estado === 'listo' && consulta.resultado === 'ENCONTRADO' && consulta.datos && (
            <>
              <TarjetaRuc ruc={ruc} datos={consulta.datos} />
              <span className="registro-campo-info">Te sugerimos estos datos según tu RUC. Puedes cambiarlos.</span>
            </>
          )}
        </div>

        <div className="registro-campo">
          <label className="registro-etiqueta" htmlFor="completar-nombre">Nombre del negocio</label>
          <input
            id="completar-nombre"
            type="text"
            placeholder="Nombre del negocio"
            value={nombre}
            onChange={(e) => setNombre(e.target.value)}
            onBlur={() => setTocados((t) => ({ ...t, nombre: true }))}
            aria-invalid={Boolean(tocados.nombre && errorDeNombre)}
          />
          {tocados.nombre && errorDeNombre && <span className="registro-campo-error">{errorDeNombre}</span>}
        </div>

        <div className="registro-campo">
          <label className="registro-etiqueta" htmlFor="completar-direccion">Dirección</label>
          <input
            id="completar-direccion"
            type="text"
            placeholder="Dirección del negocio"
            value={direccion}
            onChange={(e) => setDireccion(e.target.value)}
          />
        </div>

        <button type="submit" disabled={!puedeGuardar}>
          {guardando ? 'Guardando...' : 'Guardar y continuar'}
        </button>
        {mensaje && <p className="login-mensaje-error">{mensaje}</p>}

        <button type="button" className="login-enlace" onClick={onLogout}>
          Cerrar sesión
        </button>
      </form>
    </div>
  );
}

import { useState, useEffect, useRef } from 'react';
import { api, API_URL } from '../../api/api';
import { COLOR_MONSPEET, normalizarAcento } from '../../utils/tema';
import './Configuracion.css';
import { confirmar } from '../../utils/confirmar';
import { comprimirImagen } from '../../utils/comprimirImagen';
import ConfiguracionRestaurante from './ConfiguracionRestaurante';
import ConfiguracionUnidades from './ConfiguracionUnidades';
import ConfiguracionNegocio from './ConfiguracionNegocio';
import SelectorTasaIgv from './SelectorTasaIgv';
import ConfiguracionDetraccion from './ConfiguracionDetraccion';
import ConfiguracionCambios from './ConfiguracionCambios';
import ConfiguracionImpresora from './ConfiguracionImpresora';
import UsoPlan from '../../components/UsoPlan';
import MiCuenta from '../../components/MiCuenta';
import { etiquetaTasa, TASA_GENERAL } from '../../utils/igv';
import { datosNegocio, MODULO_CAMBIOS, MODULO_DETRACCION, MODULO_MESAS } from '../../utils/rubros';
import TarjetaRuc from '../../components/TarjetaRuc';
import { useConsultaRuc } from '../../utils/useConsultaRuc';
import { sugerirEn } from '../../utils/sugerencia';
import { datoReal, errorNombre, errorRucConfiguracion } from '../../utils/validacionRegistro';

const ROLES = [
  { id: 1, nombre: 'Administrador' },
  { id: 2, nombre: 'Cajero' },
  { id: 3, nombre: 'Almacén (productos, proveedores y reportes)' },
];

// Nombre para mostrar de cada rol que manda el servidor (/roles). El id
// del Mesero puede variar entre negocios, por eso la lista viene de ahí.
const NOMBRE_ROL = {
  ADMIN: 'Administrador',
  CAJERO: 'Cajero',
  INVENTARIO: 'Almacén (productos, proveedores y reportes)',
  MESERO: 'Mesero (toma pedidos, no cobra)',
  PREPARACION: 'Barra / Cocina (marca los pedidos listos)',
};

export default function Configuracion({ onIdentidadActualizada, usuarioActualId, onNegocioCambiado }) {
  const [roles, setRoles] = useState(ROLES);
  const [vista, setVista] = useState('NEGOCIO');

  const [formConfig, setFormConfig] = useState(null);
  const [guardandoConfig, setGuardandoConfig] = useState(false);
  const [subiendoLogo, setSubiendoLogo] = useState(false);

  const [usuarios, setUsuarios] = useState([]);
  const [cargandoUsuarios, setCargandoUsuarios] = useState(true);
  const [mostrarNuevoUsuario, setMostrarNuevoUsuario] = useState(false);
  const [formUsuario, setFormUsuario] = useState({ username: '', password: '', nombre_completo: '', rol_id: 2 });
  const [guardandoUsuario, setGuardandoUsuario] = useState(false);
  // Restablecer la contraseña de un trabajador (el usuario elegido o null).
  const [restablecer, setRestablecer] = useState(null);
  const [claveNueva, setClaveNueva] = useState('');
  const [guardandoClave, setGuardandoClave] = useState(false);

  const [mensaje, setMensaje] = useState(null);
  // Tasa de IGV que tiene guardada el negocio (para avisar si la cambia).
  const [tasaGuardada, setTasaGuardada] = useState(null);
  // RUC guardado: solo un RUC nuevo se consulta en SUNAT. Y la última
  // dirección sugerida por esa consulta (si el usuario la cambió, se respeta).
  const [rucGuardado, setRucGuardado] = useState('');
  const direccionSugerida = useRef({ direccion: '' });

  const cargarConfig = () => {
    api
      .configuracionObtener()
      .then((c) => {
        setFormConfig(c);
        setTasaGuardada(Number(c.iva_porcentaje));
        setRucGuardado(c.ruc || '');
        // La dirección guardada cuenta como sugerida: si cambia el RUC, la
        // reemplaza la del RUC nuevo (salvo que el usuario la edite antes).
        direccionSugerida.current.direccion = c.direccion || '';
      })
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }));
  };

  const cargarUsuarios = () => {
    setCargandoUsuarios(true);
    api
      .usuariosListar()
      .then(setUsuarios)
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }))
      .finally(() => setCargandoUsuarios(false));
  };

  const cargarRoles = () => {
    api
      .roles()
      .then((lista) => setRoles(lista.map((r) => ({ id: r.id, nombre: NOMBRE_ROL[r.nombre] || r.nombre }))))
      .catch(() => {});
  };

  useEffect(() => {
    cargarConfig();
    cargarUsuarios();
    cargarRoles();
  }, []);

  const negocio = datosNegocio(formConfig);
  const restaurante = negocio.modulos.includes(MODULO_MESAS);

  // RUC y nombre con las mismas reglas que el registro (el backend también
  // las revisa). Un RUC nuevo se consulta y su dirección se sugiere.
  const rucEscrito = formConfig?.ruc || '';
  const consultaRuc = useConsultaRuc(rucEscrito, vista === 'NEGOCIO' && rucEscrito !== rucGuardado, (datos) => {
    const setDireccion = (cambio) => setFormConfig((actual) => ({ ...actual, direccion: cambio(actual?.direccion) }));
    sugerirEn(setDireccion, direccionSugerida, 'direccion', datoReal(datos.direccion));
  });
  const errorRuc = formConfig ? errorRucConfiguracion(rucGuardado, rucEscrito) : null;
  const rucNoExiste = consultaRuc.estado === 'listo' && consultaRuc.resultado === 'NO_EXISTE';
  const errorNombreNegocio = formConfig?.nombre_tienda?.trim()
    ? errorNombre(formConfig.nombre_tienda, 'El nombre del negocio')
    : null;

  // Configuración → Rubro y módulos guardó un cambio.
  const alCambiarNegocio = (cambio) => {
    setFormConfig((actual) => ({ ...actual, ...cambio }));
    onNegocioCambiado?.(cambio);
    cargarRoles();
  };

  const guardarConfig = async () => {
    setMensaje(null);
    if (!formConfig.nombre_tienda.trim()) {
      setMensaje({ tipo: 'error', texto: 'El nombre de la tienda es obligatorio.' });
      return;
    }
    if (errorNombreNegocio || errorRuc || rucNoExiste) {
      setMensaje({ tipo: 'error', texto: errorNombreNegocio || errorRuc || consultaRuc.mensaje });
      return;
    }
    if (consultaRuc.estado === 'consultando') {
      setMensaje({ tipo: 'error', texto: 'Espera a que termine la consulta del RUC.' });
      return;
    }
    // La tasa de IGV se usa de verdad al cobrar y al emitir comprobantes.
    const tasa = Number(formConfig.iva_porcentaje);
    if (!(tasa > 0 && tasa <= 30)) {
      setMensaje({ tipo: 'error', texto: 'Escribe una tasa de IGV válida (mayor a 0 y hasta 30 %).' });
      return;
    }
    if (tasaGuardada !== null && tasa !== tasaGuardada) {
      const ok = await confirmar({
        titulo: `¿Cambiar el IGV a ${etiquetaTasa(tasa)} %?`,
        mensaje:
          tasa === TASA_GENERAL
            ? 'Desde ahora tus ventas y comprobantes se calcularán con la tasa general de 18 %. Las ventas ya hechas no cambian.'
            : `Desde ahora tus ventas y comprobantes se calcularán con ${etiquetaTasa(tasa)} %. Hazlo solo si tu negocio está inscrito en el padrón de SUNAT para esa tasa; si no, tus comprobantes saldrán observados. Las ventas ya hechas no cambian.`,
        textoConfirmar: 'Cambiar tasa',
        icono: 'aviso',
      });
      if (!ok) return;
    }
    setGuardandoConfig(true);
    try {
      await api.configuracionActualizar({
        nombre_tienda: formConfig.nombre_tienda.trim(),
        direccion: formConfig.direccion || null,
        telefono: formConfig.telefono || null,
        email: formConfig.email || null,
        ruc: formConfig.ruc || null,
        moneda: 'PEN',
        iva_porcentaje: tasa,
        facturalibre_token: formConfig.facturalibre_token || null,
        facturalibre_ruta: formConfig.facturalibre_ruta || null,
        codigo_producto_sunat_generico: formConfig.codigo_producto_sunat_generico || null,
        serie_boleta: formConfig.serie_boleta || null,
        serie_factura: formConfig.serie_factura || null,
        color_acento: formConfig.color_acento || null,
      });
      setMensaje({ tipo: 'exito', texto: 'Configuración guardada.' });
      const actualizada = await api.configuracionObtener();
      setFormConfig(actualizada);
      setTasaGuardada(Number(actualizada.iva_porcentaje));
      setRucGuardado(actualizada.ruc || '');
      onIdentidadActualizada?.({
        logo_url: actualizada.logo_path,
        color_acento: actualizada.color_acento,
      });
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardandoConfig(false);
    }
  };

  const subirLogo = async (e) => {
    const archivo = e.target.files[0];
    if (!archivo) return;

    setMensaje(null);
    setSubiendoLogo(true);
    try {
      // En PNG para conservar la transparencia; el servidor lo deja en 400 px.
      await api.configuracionSubirLogo(await comprimirImagen(archivo, { ladoMaximo: 800, png: true }));
      setMensaje({ tipo: 'exito', texto: 'Logo actualizado.' });
      const actualizada = await api.configuracionObtener();
      setFormConfig(actualizada);
      onIdentidadActualizada?.({
        logo_url: actualizada.logo_path,
        color_acento: actualizada.color_acento,
      });
    } catch (err) {
      setMensaje({ tipo: 'error', texto: err.message });
    } finally {
      setSubiendoLogo(false);
      e.target.value = '';
    }
  };

  const guardarUsuario = async () => {
    setMensaje(null);
    if (!formUsuario.username.trim() || !formUsuario.password.trim() || !formUsuario.nombre_completo.trim()) {
      setMensaje({ tipo: 'error', texto: 'Completa usuario, contraseña y nombre completo.' });
      return;
    }
    if (formUsuario.password.length < 8) {
      setMensaje({ tipo: 'error', texto: 'La contraseña debe tener al menos 8 caracteres.' });
      return;
    }
    setGuardandoUsuario(true);
    try {
      await api.usuarioCrear(formUsuario);
      setMensaje({ tipo: 'exito', texto: 'Usuario creado.' });
      setMostrarNuevoUsuario(false);
      setFormUsuario({ username: '', password: '', nombre_completo: '', rol_id: 2 });
      cargarUsuarios();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardandoUsuario(false);
    }
  };

  const abrirRestablecer = (u) => {
    setClaveNueva('');
    setRestablecer(u);
  };

  const guardarClaveNueva = async () => {
    setMensaje(null);
    if (claveNueva.length < 8) {
      setMensaje({ tipo: 'error', texto: 'La contraseña debe tener al menos 8 caracteres.' });
      return;
    }
    setGuardandoClave(true);
    try {
      await api.usuarioRestablecerClave(restablecer.id, claveNueva);
      setMensaje({
        tipo: 'exito',
        texto: `Contraseña de "${restablecer.username}" restablecida. Si tenía la sesión abierta, tendrá que volver a entrar con la nueva.`,
      });
      setRestablecer(null);
      setClaveNueva('');
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardandoClave(false);
    }
  };

  const reactivarUsuario = async (u) => {
    const confirmado = await confirmar({
      titulo: `¿Reactivar a "${u.nombre_completo}"?`,
      mensaje: `Podrá volver a iniciar sesión con su usuario "${u.username}" y su misma contraseña.`,
      textoConfirmar: 'Reactivar',
      tipo: 'normal',
      icono: 'reactivar',
    });
    if (!confirmado) return;
    try {
      await api.usuarioReactivar(u.id);
      setMensaje({ tipo: 'exito', texto: 'Usuario reactivado.' });
      cargarUsuarios();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
  };

  const desactivarUsuario = async (u) => {
    const confirmado = await confirmar({
      titulo: `¿Desactivar a "${u.nombre_completo}"?`,
      mensaje: 'Ya no podrá iniciar sesión. Sus ventas y cierres de caja se conservan.',
      textoConfirmar: 'Desactivar',
      icono: 'usuario',
    });
    if (!confirmado) return;
    try {
      await api.usuarioDesactivar(u.id);
      setMensaje({ tipo: 'exito', texto: 'Usuario desactivado.' });
      cargarUsuarios();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
  };

  if (!formConfig) {
    return (
      <div className="cfg-layout">
        <p className="cfg-cargando">Cargando...</p>
      </div>
    );
  }

  return (
    <div className="cfg-layout">
      <div className="cfg-header">
        <h1>Configuración</h1>
        <div className="cfg-tabs">
          <button className={vista === 'NEGOCIO' ? 'activo' : ''} onClick={() => setVista('NEGOCIO')}>
            Datos del negocio
          </button>
          <button className={vista === 'USUARIOS' ? 'activo' : ''} onClick={() => setVista('USUARIOS')}>
            Usuarios
          </button>
          {restaurante && (
            <button className={vista === 'RESTAURANTE' ? 'activo' : ''} onClick={() => setVista('RESTAURANTE')}>
              Mesas y opciones
            </button>
          )}
          <button className={vista === 'UNIDADES' ? 'activo' : ''} onClick={() => setVista('UNIDADES')}>
            Unidades
          </button>
          <button className={vista === 'PLAN' ? 'activo' : ''} onClick={() => setVista('PLAN')}>
            Plan y comprobantes
          </button>
          <button className={vista === 'MI_CUENTA' ? 'activo' : ''} onClick={() => setVista('MI_CUENTA')}>
            Mi contraseña
          </button>
        </div>
      </div>

      {mensaje && <p className={`cfg-mensaje cfg-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

      {vista === 'NEGOCIO' && formConfig && (
        <ConfiguracionNegocio
          rubro={negocio.rubro}
          modulos={negocio.modulos}
          onCambiado={alCambiarNegocio}
          onMensaje={setMensaje}
        />
      )}

      {vista === 'NEGOCIO' && formConfig && negocio.modulos.includes(MODULO_DETRACCION) && (
        <ConfiguracionDetraccion onMensaje={setMensaje} />
      )}

      {vista === 'NEGOCIO' && formConfig && negocio.modulos.includes(MODULO_CAMBIOS) && (
        <ConfiguracionCambios onMensaje={setMensaje} />
      )}

      {vista === 'NEGOCIO' && formConfig && <ConfiguracionImpresora onMensaje={setMensaje} />}

      {vista === 'RESTAURANTE' && restaurante && <ConfiguracionRestaurante />}
      {vista === 'UNIDADES' && <ConfiguracionUnidades rubro={negocio.rubro} />}
      {vista === 'PLAN' && <UsoPlan />}
      {vista === 'MI_CUENTA' && <MiCuenta onMensaje={setMensaje} />}

      {vista === 'NEGOCIO' && (
        <div className="cfg-card">
          <div className="cfg-campo">
            <label>Nombre del negocio</label>
            <input
              value={formConfig.nombre_tienda}
              onChange={(e) => setFormConfig({ ...formConfig, nombre_tienda: e.target.value })}
              aria-invalid={Boolean(errorNombreNegocio)}
            />
            {errorNombreNegocio && <span className="cfg-campo-error">{errorNombreNegocio}</span>}
          </div>

          <div className="cfg-campo">
            <label>RUC</label>
            <input
              inputMode="numeric"
              maxLength={11}
              value={formConfig.ruc || ''}
              onChange={(e) => setFormConfig({ ...formConfig, ruc: e.target.value.replace(/\D/g, '').slice(0, 11) })}
              aria-invalid={Boolean(errorRuc || rucNoExiste)}
            />
            {errorRuc && <span className="cfg-campo-error">{errorRuc}</span>}
            {!errorRuc && consultaRuc.estado === 'consultando' && (
              <span className="cfg-campo-info">Consultando SUNAT...</span>
            )}
            {rucNoExiste && <span className="cfg-campo-error">{consultaRuc.mensaje}</span>}
            {consultaRuc.estado === 'listo' && consultaRuc.resultado === 'NO_DISPONIBLE' && (
              <span className="cfg-campo-info">{consultaRuc.mensaje}</span>
            )}
            {consultaRuc.estado === 'listo' && consultaRuc.resultado === 'ENCONTRADO' && consultaRuc.datos && (
              <TarjetaRuc ruc={rucEscrito} datos={consultaRuc.datos} />
            )}
          </div>

          <div className="cfg-campo">
            <label>Dirección</label>
            <input
              value={formConfig.direccion || ''}
              onChange={(e) => setFormConfig({ ...formConfig, direccion: e.target.value })}
            />
          </div>

          <div className="cfg-campo-fila">
            <div className="cfg-campo">
              <label>Teléfono</label>
              <input
                value={formConfig.telefono || ''}
                onChange={(e) => setFormConfig({ ...formConfig, telefono: e.target.value })}
              />
            </div>
            <div className="cfg-campo">
              <label>Email</label>
              <input value={formConfig.email || ''} onChange={(e) => setFormConfig({ ...formConfig, email: e.target.value })} />
            </div>
          </div>

          <div className="cfg-campo-fila">
            <div className="cfg-campo">
              <label>Moneda</label>
              <div className="cfg-valor-fijo">Soles (S/)</div>
              <p className="cfg-nota-moneda">
                El sistema opera únicamente en soles. No se puede cambiar porque el tipo de cambio no se
                registra por transacción — hacerlo generaría descuadres en caja y reportes históricos.
              </p>
            </div>
          </div>

          <SelectorTasaIgv
            valor={formConfig.iva_porcentaje}
            onCambiar={(tasa) => setFormConfig({ ...formConfig, iva_porcentaje: tasa })}
          />

          <div className="cfg-separador-seccion"></div>
          <h3 className="cfg-subtitulo-seccion">Identidad visual del negocio</h3>
          <p className="cfg-nota-moneda">
            El logo y el color se aplican en todo tu sistema: menú lateral, botones, pantalla de inicio de
            sesión de tus cajeros y demás pantallas. Solo afectan a tu negocio.
          </p>

          <div className="cfg-campo">
            <label>Logo del negocio</label>
            {formConfig.logo_path && (
              <img
                src={`${API_URL}${formConfig.logo_path}?t=${Date.now()}`}
                alt="Logo actual"
                style={{ width: 72, height: 72, objectFit: 'cover', borderRadius: 12, marginBottom: 8, display: 'block' }}
              />
            )}
            <input type="file" accept="image/*" onChange={subirLogo} disabled={subiendoLogo} />
            {subiendoLogo && <p className="cfg-nota-moneda">Subiendo...</p>}
          </div>

          <div className="cfg-campo">
            <label>Color de acento</label>
            <div className="cfg-color-fila">
              <input
                type="color"
                value={normalizarAcento(formConfig.color_acento)}
                onChange={(e) => setFormConfig({ ...formConfig, color_acento: e.target.value })}
                style={{ width: 60, height: 36, padding: 2, cursor: 'pointer' }}
              />
              {normalizarAcento(formConfig.color_acento) !== COLOR_MONSPEET && (
                <button
                  type="button"
                  className="cfg-boton-color-original"
                  onClick={() => setFormConfig({ ...formConfig, color_acento: COLOR_MONSPEET })}
                >
                  Usar verde Monspeet
                </button>
              )}
            </div>
            <p className="cfg-nota-moneda">Se aplica al presionar «Guardar cambios».</p>
          </div>

          <div className="cfg-separador-seccion"></div>
          <h3 className="cfg-subtitulo-seccion">Facturación electrónica — FacturaLibre</h3>
          <p className="cfg-nota-moneda">
            Boleta y Factura se emiten a través de FacturaLibre.org. Consigue tu Token y URL en el panel de
            tu cuenta y pégalos aquí. Sin estos datos, el POS no podrá emitir Boleta ni Factura (Nota simple
            sigue funcionando siempre, ya que no necesita comprobante tributario).
          </p>

          <div className="cfg-campo">
            <label>Token de FacturaLibre</label>
            <input
              type="password"
              value={formConfig.facturalibre_token || ''}
              onChange={(e) => setFormConfig({ ...formConfig, facturalibre_token: e.target.value })}
              placeholder="Token de tu cuenta de FacturaLibre"
            />
          </div>

          <div className="cfg-campo">
            <label>URL / Ruta de FacturaLibre</label>
            <input
              value={formConfig.facturalibre_ruta || ''}
              onChange={(e) => setFormConfig({ ...formConfig, facturalibre_ruta: e.target.value })}
              placeholder="https://..."
            />
          </div>

          <div className="cfg-campo-fila">
            <div className="cfg-campo">
              <label>Serie de Boleta</label>
              <input
                value={formConfig.serie_boleta || ''}
                onChange={(e) => setFormConfig({ ...formConfig, serie_boleta: e.target.value })}
                placeholder="B001"
              />
            </div>
            <div className="cfg-campo">
              <label>Serie de Factura</label>
              <input
                value={formConfig.serie_factura || ''}
                onChange={(e) => setFormConfig({ ...formConfig, serie_factura: e.target.value })}
                placeholder="F001"
              />
            </div>
          </div>
          <p className="cfg-nota-moneda">
            Deben coincidir exactamente con las series activas en tu cuenta de FacturaLibre. Si las dejas
            vacías, el sistema usa B001/F001 por defecto.
          </p>

          <div className="cfg-campo">
            <label>Código de producto SUNAT (genérico)</label>
            <input
              value={formConfig.codigo_producto_sunat_generico || ''}
              onChange={(e) => setFormConfig({ ...formConfig, codigo_producto_sunat_generico: e.target.value })}
              placeholder="50000000"
            />
          </div>
          <p className="cfg-nota-moneda">
            Código de 8 dígitos del Catálogo N° 25 de SUNAT que se aplica a todos tus productos. Solo es
            obligatorio para categorías específicas de riesgo (combustibles, oro, insumos químicos, entre
            otras) desde el 1 de enero de 2027 — si no vendes ese tipo de productos, puedes dejar el valor
            por defecto.
          </p>

          <button className="cfg-boton-guardar" onClick={guardarConfig} disabled={guardandoConfig}>
            {guardandoConfig ? 'Guardando...' : 'Guardar cambios'}
          </button>
        </div>
      )}

      {vista === 'USUARIOS' && (
        <>
          <button className="cfg-boton-nuevo-usuario" onClick={() => setMostrarNuevoUsuario(true)}>
            + Nuevo usuario
          </button>

          {cargandoUsuarios ? (
            <p className="cfg-cargando">Cargando...</p>
          ) : (
            <div className="cfg-tabla-wrapper">
              <table className="cfg-tabla">
                <thead>
                  <tr>
                    <th>Usuario</th>
                    <th>Nombre completo</th>
                    <th>Rol</th>
                    <th>Estado</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {usuarios.map((u) => (
                    <tr key={u.id}>
                      <td>{u.username}</td>
                      <td>{u.nombre_completo}</td>
                      <td>{u.rol_nombre === 'INVENTARIO' ? 'ALMACÉN' : u.rol_nombre}</td>
                      <td>
                        <span className={`cfg-badge ${u.activo ? 'cfg-badge-activo' : 'cfg-badge-inactivo'}`}>
                          {u.activo ? 'Activo' : 'Inactivo'}
                        </span>
                      </td>
                      <td>
                        {u.activo && u.id !== usuarioActualId && (
                          <span className="cfg-acciones-usuario">
                            <button className="cfg-boton-clave" onClick={() => abrirRestablecer(u)}>
                              Contraseña
                            </button>
                            <button className="cfg-boton-desactivar" onClick={() => desactivarUsuario(u)}>
                              Desactivar
                            </button>
                          </span>
                        )}
                        {u.activo && u.id === usuarioActualId && <span className="cfg-tu-usuario">Tu usuario</span>}
                        {!u.activo && (
                          <button className="cfg-boton-reactivar" onClick={() => reactivarUsuario(u)}>
                            Reactivar
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {mostrarNuevoUsuario && (
            <div className="cfg-modal-overlay" onClick={() => setMostrarNuevoUsuario(false)}>
              <div className="cfg-modal" onClick={(e) => e.stopPropagation()}>
                <h2>Nuevo usuario</h2>

                <div className="cfg-campo">
                  <label>Nombre de usuario (para iniciar sesión)</label>
                  <input
                    value={formUsuario.username}
                    onChange={(e) => setFormUsuario({ ...formUsuario, username: e.target.value })}
                  />
                </div>
                <div className="cfg-campo">
                  <label>Contraseña (mínimo 8 caracteres)</label>
                  <input
                    type="password"
                    value={formUsuario.password}
                    onChange={(e) => setFormUsuario({ ...formUsuario, password: e.target.value })}
                  />
                </div>
                <div className="cfg-campo">
                  <label>Nombre completo</label>
                  <input
                    value={formUsuario.nombre_completo}
                    onChange={(e) => setFormUsuario({ ...formUsuario, nombre_completo: e.target.value })}
                  />
                </div>
                <div className="cfg-campo">
                  <label>Rol</label>
                  <select
                    value={formUsuario.rol_id}
                    onChange={(e) => setFormUsuario({ ...formUsuario, rol_id: parseInt(e.target.value, 10) })}
                  >
                    {roles.map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.nombre}
                      </option>
                    ))}
                  </select>
                </div>

                <div className="cfg-modal-acciones">
                  <button className="cfg-boton-cancelar" onClick={() => setMostrarNuevoUsuario(false)}>
                    Cancelar
                  </button>
                  <button className="cfg-boton-guardar-modal" onClick={guardarUsuario} disabled={guardandoUsuario}>
                    {guardandoUsuario ? 'Creando...' : 'Crear usuario'}
                  </button>
                </div>
              </div>
            </div>
          )}

          {restablecer && (
            <div className="cfg-modal-overlay" onClick={() => setRestablecer(null)}>
              <div className="cfg-modal" onClick={(e) => e.stopPropagation()}>
                <h2>Contraseña de {restablecer.nombre_completo}</h2>
                <p className="cfg-nota-clave">
                  Escribe una contraseña nueva para <strong>{restablecer.username}</strong> y pásasela. Si tenía la sesión
                  abierta en algún dispositivo, se cerrará.
                </p>
                <div className="cfg-campo">
                  <label>Contraseña nueva (mínimo 8 caracteres)</label>
                  <input
                    type="text"
                    autoComplete="off"
                    autoFocus
                    value={claveNueva}
                    onChange={(e) => setClaveNueva(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && guardarClaveNueva()}
                  />
                </div>
                <div className="cfg-modal-acciones">
                  <button className="cfg-boton-cancelar" onClick={() => setRestablecer(null)}>
                    Cancelar
                  </button>
                  <button className="cfg-boton-guardar-modal" onClick={guardarClaveNueva} disabled={guardandoClave}>
                    {guardandoClave ? 'Guardando...' : 'Guardar contraseña'}
                  </button>
                </div>
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}
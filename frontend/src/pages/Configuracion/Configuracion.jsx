import { useState, useEffect } from 'react';
import { api } from '../../api/api';
import './Configuracion.css';

const ROLES = [
  { id: 1, nombre: 'Administrador' },
  { id: 2, nombre: 'Cajero' },
  { id: 3, nombre: 'Inventario' },
];

export default function Configuracion() {
  const [vista, setVista] = useState('NEGOCIO');

  const [formConfig, setFormConfig] = useState(null);
  const [guardandoConfig, setGuardandoConfig] = useState(false);

  const [usuarios, setUsuarios] = useState([]);
  const [cargandoUsuarios, setCargandoUsuarios] = useState(true);
  const [mostrarNuevoUsuario, setMostrarNuevoUsuario] = useState(false);
  const [formUsuario, setFormUsuario] = useState({ username: '', password: '', nombre_completo: '', rol_id: 2 });
  const [guardandoUsuario, setGuardandoUsuario] = useState(false);

  const [mensaje, setMensaje] = useState(null);

  const cargarConfig = () => {
    api
      .configuracionObtener()
      .then((c) => setFormConfig(c))
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

  useEffect(() => {
    cargarConfig();
    cargarUsuarios();
  }, []);

  const guardarConfig = async () => {
    setMensaje(null);
    if (!formConfig.nombre_tienda.trim()) {
      setMensaje({ tipo: 'error', texto: 'El nombre de la tienda es obligatorio.' });
      return;
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
        iva_porcentaje: parseFloat(formConfig.iva_porcentaje) || 18,
        facturalibre_token: formConfig.facturalibre_token || null,
        facturalibre_ruta: formConfig.facturalibre_ruta || null,
        codigo_producto_sunat_generico: null,
      });
      setMensaje({ tipo: 'exito', texto: 'Configuración guardada.' });
      cargarConfig();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardandoConfig(false);
    }
  };

  const guardarUsuario = async () => {
    setMensaje(null);
    if (!formUsuario.username.trim() || !formUsuario.password.trim() || !formUsuario.nombre_completo.trim()) {
      setMensaje({ tipo: 'error', texto: 'Completa usuario, contraseña y nombre completo.' });
      return;
    }
    if (formUsuario.password.length < 6) {
      setMensaje({ tipo: 'error', texto: 'La contraseña debe tener al menos 6 caracteres.' });
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

  const desactivarUsuario = async (u) => {
    if (!confirm(`¿Desactivar a "${u.nombre_completo}"? Ya no podrá iniciar sesión.`)) return;
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
        </div>
      </div>

      {mensaje && <p className={`cfg-mensaje cfg-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

      {vista === 'NEGOCIO' && (
        <div className="cfg-card">
          <div className="cfg-campo">
            <label>Nombre del negocio</label>
            <input
              value={formConfig.nombre_tienda}
              onChange={(e) => setFormConfig({ ...formConfig, nombre_tienda: e.target.value })}
            />
          </div>

          <div className="cfg-campo">
            <label>RUC</label>
            <input value={formConfig.ruc || ''} onChange={(e) => setFormConfig({ ...formConfig, ruc: e.target.value })} />
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
            <div className="cfg-campo">
              <label>IGV (%)</label>
              <input
                type="number"
                value={formConfig.iva_porcentaje}
                onChange={(e) => setFormConfig({ ...formConfig, iva_porcentaje: e.target.value })}
              />
            </div>
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
                      <td>{u.rol_nombre}</td>
                      <td>
                        <span className={`cfg-badge ${u.activo ? 'cfg-badge-activo' : 'cfg-badge-inactivo'}`}>
                          {u.activo ? 'Activo' : 'Inactivo'}
                        </span>
                      </td>
                      <td>
                        {u.activo && (
                          <button className="cfg-boton-desactivar" onClick={() => desactivarUsuario(u)}>
                            Desactivar
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
                  <label>Contraseña</label>
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
                    {ROLES.map((r) => (
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
        </>
      )}
    </div>
  );
}
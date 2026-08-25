import { useState, useEffect, useMemo } from 'react';
import { api } from '../../api/api';
import './Clientes.css';

const REGLAS_DOCUMENTO = {
  DNI: { maxLength: 8, soloNumeros: true, label: 'DNI (8 dígitos)' },
  RUC: { maxLength: 11, soloNumeros: true, label: 'RUC (11 dígitos)' },
  CE: { maxLength: 12, soloNumeros: false, label: 'Carnet de Extranjería' },
  PASAPORTE: { maxLength: 12, soloNumeros: false, label: 'Pasaporte' },
  SIN_DOCUMENTO: { maxLength: 0, soloNumeros: false, label: 'Sin documento' },
};

const FORM_VACIO = {
  tipo_documento: 'DNI',
  numero_documento: '',
  nombre_razon_social: '',
  telefono: '',
  email: '',
  direccion: '',
};

export default function Clientes() {
  const [clientes, setClientes] = useState([]);
  const [busqueda, setBusqueda] = useState('');
  const [cargando, setCargando] = useState(true);
  const [mensaje, setMensaje] = useState(null);

  const [mostrarForm, setMostrarForm] = useState(false);
  const [editandoId, setEditandoId] = useState(null);
  const [form, setForm] = useState(FORM_VACIO);
  const [guardando, setGuardando] = useState(false);

  const cargarClientes = () => {
    setCargando(true);
    api
      .clientesTodos()
      .then(setClientes)
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }))
      .finally(() => setCargando(false));
  };

  useEffect(() => {
    cargarClientes();
  }, []);

  const clientesFiltrados = useMemo(() => {
    if (!busqueda.trim()) return clientes;
    const q = busqueda.toLowerCase();
    return clientes.filter(
      (c) =>
        c.nombre_razon_social.toLowerCase().includes(q) ||
        (c.numero_documento || '').includes(busqueda)
    );
  }, [clientes, busqueda]);

  const abrirNuevo = () => {
    setEditandoId(null);
    setForm(FORM_VACIO);
    setMensaje(null);
    setMostrarForm(true);
  };

  const abrirEdicion = (c) => {
    setEditandoId(c.id);
    setForm({
      tipo_documento: c.tipo_documento,
      numero_documento: c.numero_documento || '',
      nombre_razon_social: c.nombre_razon_social,
      telefono: c.telefono || '',
      email: c.email || '',
      direccion: c.direccion || '',
    });
    setMensaje(null);
    setMostrarForm(true);
  };

  const cerrarForm = () => {
    setMostrarForm(false);
    setEditandoId(null);
    setForm(FORM_VACIO);
  };

  const cambiarTipoDocumento = (tipo) => {
    const regla = REGLAS_DOCUMENTO[tipo];
    let limpio = form.numero_documento;
    if (regla.soloNumeros) limpio = limpio.replace(/\D/g, '');
    limpio = limpio.slice(0, regla.maxLength || limpio.length);
    setForm((f) => ({ ...f, tipo_documento: tipo, numero_documento: tipo === 'SIN_DOCUMENTO' ? '' : limpio }));
  };

  const cambiarDocumento = (valor) => {
    const regla = REGLAS_DOCUMENTO[form.tipo_documento];
    let limpio = valor;
    if (regla.soloNumeros) limpio = limpio.replace(/\D/g, '');
    limpio = limpio.slice(0, regla.maxLength);
    setForm((f) => ({ ...f, numero_documento: limpio }));
  };

  const guardar = async () => {
    setMensaje(null);
    if (!form.nombre_razon_social.trim()) {
      setMensaje({ tipo: 'error', texto: 'El nombre es obligatorio.' });
      return;
    }
    const regla = REGLAS_DOCUMENTO[form.tipo_documento];
    if (form.tipo_documento !== 'SIN_DOCUMENTO' && regla.soloNumeros && form.numero_documento.length !== regla.maxLength) {
      setMensaje({ tipo: 'error', texto: `${regla.label} debe tener exactamente ${regla.maxLength} dígitos.` });
      return;
    }

    const payload = {
      tipo_documento: form.tipo_documento,
      numero_documento: form.numero_documento || null,
      nombre_razon_social: form.nombre_razon_social.trim(),
      telefono: form.telefono || null,
      email: form.email || null,
      direccion: form.direccion || null,
    };

    setGuardando(true);
    try {
      if (editandoId) {
        await api.clienteActualizar(editandoId, payload);
        setMensaje({ tipo: 'exito', texto: 'Cliente actualizado.' });
      } else {
        await api.clienteCrear(payload);
        setMensaje({ tipo: 'exito', texto: 'Cliente registrado.' });
      }
      cerrarForm();
      cargarClientes();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  const desactivar = async (c) => {
    if (!confirm(`¿Desactivar a "${c.nombre_razon_social}"? Ya no aparecerá en las búsquedas del POS.`)) return;
    try {
      await api.clienteDesactivar(c.id);
      setMensaje({ tipo: 'exito', texto: 'Cliente desactivado.' });
      cargarClientes();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
  };

  const regla = REGLAS_DOCUMENTO[form.tipo_documento];

  return (
    <div className="cli-layout">
      <div className="cli-header">
        <h1>Clientes</h1>
        <button className="cli-boton-nuevo" onClick={abrirNuevo}>
          + Nuevo cliente
        </button>
      </div>

      <input
        className="cli-buscador"
        type="text"
        placeholder="Buscar por nombre o documento..."
        value={busqueda}
        onChange={(e) => setBusqueda(e.target.value)}
      />

      {mensaje && !mostrarForm && (
        <p className={`cli-mensaje cli-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>
      )}

      {cargando ? (
        <p className="cli-cargando">Cargando...</p>
      ) : (
        <div className="cli-tabla-wrapper">
          <table className="cli-tabla">
            <thead>
              <tr>
                <th>Nombre / Razón social</th>
                <th>Documento</th>
                <th>Teléfono</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {clientesFiltrados.map((c) => (
                <tr key={c.id}>
                  <td>{c.nombre_razon_social}</td>
                  <td>
                    {c.numero_documento ? (
                      <>
                        <span className="cli-badge-tipo">{c.tipo_documento}</span> {c.numero_documento}
                      </>
                    ) : (
                      '—'
                    )}
                  </td>
                  <td>{c.telefono || '—'}</td>
                  <td>
                    <button className="cli-boton-editar" onClick={() => abrirEdicion(c)}>Editar</button>
                    <button className="cli-boton-eliminar" onClick={() => desactivar(c)}>Desactivar</button>
                  </td>
                </tr>
              ))}
              {clientesFiltrados.length === 0 && (
                <tr>
                  <td colSpan={4} className="cli-sin-resultados">
                    {busqueda ? 'No hay clientes que coincidan.' : 'Aún no hay clientes registrados.'}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {mostrarForm && (
        <div className="cli-modal-overlay" onClick={cerrarForm}>
          <div className="cli-modal" onClick={(e) => e.stopPropagation()}>
            <h2>{editandoId ? 'Editar cliente' : 'Nuevo cliente'}</h2>

            <div className="cli-campo">
              <label>Tipo de documento</label>
              <div className="cli-tipo-opciones">
                {Object.keys(REGLAS_DOCUMENTO).map((tipo) => (
                  <button
                    key={tipo}
                    className={form.tipo_documento === tipo ? 'activo' : ''}
                    onClick={() => cambiarTipoDocumento(tipo)}
                  >
                    {tipo === 'SIN_DOCUMENTO' ? 'Ninguno' : tipo}
                  </button>
                ))}
              </div>
            </div>

            {form.tipo_documento !== 'SIN_DOCUMENTO' && (
              <div className="cli-campo">
                <label>{regla.label}</label>
                <input
                  value={form.numero_documento}
                  onChange={(e) => cambiarDocumento(e.target.value)}
                  inputMode={regla.soloNumeros ? 'numeric' : 'text'}
                />
              </div>
            )}

            <div className="cli-campo">
              <label>{form.tipo_documento === 'RUC' ? 'Razón social' : 'Nombre completo'}</label>
              <input
                value={form.nombre_razon_social}
                onChange={(e) => setForm((f) => ({ ...f, nombre_razon_social: e.target.value }))}
              />
            </div>

            <div className="cli-campo-fila">
              <div className="cli-campo">
                <label>Teléfono (opcional)</label>
                <input value={form.telefono} onChange={(e) => setForm((f) => ({ ...f, telefono: e.target.value }))} />
              </div>
              <div className="cli-campo">
                <label>Email (opcional)</label>
                <input value={form.email} onChange={(e) => setForm((f) => ({ ...f, email: e.target.value }))} />
              </div>
            </div>

            <div className="cli-campo">
              <label>Dirección (opcional)</label>
              <input value={form.direccion} onChange={(e) => setForm((f) => ({ ...f, direccion: e.target.value }))} />
            </div>

            {mensaje && <p className={`cli-mensaje cli-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

            <div className="cli-modal-acciones">
              <button className="cli-boton-cancelar" onClick={cerrarForm}>Cancelar</button>
              <button className="cli-boton-guardar" onClick={guardar} disabled={guardando}>
                {guardando ? 'Guardando...' : editandoId ? 'Guardar cambios' : 'Registrar cliente'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
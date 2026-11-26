import { useState } from 'react';
import { panelApi } from '../../api/panel';
import { MODULO_MESAS, RUBROS } from '../../utils/rubros';

const VACIO = { nombre_negocio: '', nombre_completo: '', usuario: '', password: '', ruc: '' };

/**
 * Crear un negocio desde el panel (POST /panel/negocios). Igual que el
 * registro, pero el RUC es opcional: sin RUC el negocio trabaja solo con
 * tickets (notas de venta); su RUC se pone en Configuración el día que pase a
 * SUNAT. Si se escribe un RUC, el servidor lo valida como en el registro.
 */
export default function PanelNuevoNegocio({ onCreado, onCerrar }) {
  const [datos, setDatos] = useState(VACIO);
  const [rubro, setRubro] = useState(RUBROS[0].valor);
  const [guardando, setGuardando] = useState(false);
  const [error, setError] = useState('');
  const [creado, setCreado] = useState(null);

  const cambiar = (campo) => (e) => setDatos((d) => ({ ...d, [campo]: e.target.value }));
  const valido =
    datos.nombre_negocio.trim() && datos.nombre_completo.trim() && datos.usuario.trim() && datos.password.length >= 6;

  const crear = async (e) => {
    e.preventDefault();
    if (!valido || guardando) return;
    setGuardando(true);
    setError('');
    try {
      const r = await panelApi.crearNegocio({
        nombre_negocio: datos.nombre_negocio.trim(),
        nombre_completo: datos.nombre_completo.trim(),
        usuario: datos.usuario.trim(),
        password: datos.password,
        ruc: datos.ruc.trim() || null,
        rubro,
        modo_negocio: RUBROS.find((x) => x.valor === rubro)?.modulos.includes(MODULO_MESAS) ? 'RESTAURANTE' : 'TIENDA',
      });
      setCreado({ ...r, usuario: datos.usuario.trim(), nombre: datos.nombre_negocio.trim(), sinRuc: !datos.ruc.trim() });
      setDatos(VACIO);
      onCreado?.();
    } catch (e2) {
      setError(e2.message);
    } finally {
      setGuardando(false);
    }
  };

  return (
    <div className="pnl-caja">
      <h3>Crear negocio</h3>
      <p className="pnl-tenue">
        El RUC es opcional: sin RUC el negocio trabaja solo con tickets (notas de venta). Se le pone su RUC en
        Configuración el día que quiera boletas y facturas.
      </p>

      {creado && (
        <div className="pnl-aviso pnl-aviso-ok">
          <p>
            Listo: <strong>{creado.nombre}</strong> creado{creado.sinRuc ? ' sin RUC' : ''}. Usuario:{' '}
            <strong>{creado.usuario}</strong>
            {creado.identificador ? ` · ${creado.identificador}` : ''}.
          </p>
        </div>
      )}
      {error && (
        <div className="pnl-aviso pnl-aviso-error">
          <p>{error}</p>
        </div>
      )}

      <form className="pnl-formulario" onSubmit={crear}>
        <label className="pnl-campo">
          <span>Nombre del negocio</span>
          <input value={datos.nombre_negocio} onChange={cambiar('nombre_negocio')} placeholder="El Buen Camino" />
        </label>
        <label className="pnl-campo">
          <span>RUC (opcional)</span>
          <input
            value={datos.ruc}
            inputMode="numeric"
            maxLength={11}
            onChange={(e) => setDatos((d) => ({ ...d, ruc: e.target.value.replace(/\D/g, '') }))}
            placeholder="Vacío = sin RUC, solo tickets"
          />
        </label>
        <label className="pnl-campo pnl-ancho">
          <span>Rubro</span>
          <select value={rubro} onChange={(e) => setRubro(e.target.value)}>
            {RUBROS.map((r) => (
              <option key={r.valor} value={r.valor}>
                {r.label}
              </option>
            ))}
          </select>
        </label>
        <label className="pnl-campo">
          <span>Nombre del dueño (administrador)</span>
          <input value={datos.nombre_completo} onChange={cambiar('nombre_completo')} />
        </label>
        <label className="pnl-campo">
          <span>Usuario</span>
          <input value={datos.usuario} onChange={cambiar('usuario')} autoCapitalize="none" autoComplete="off" />
        </label>
        <label className="pnl-campo">
          <span>Contraseña (mínimo 6)</span>
          <input type="text" value={datos.password} onChange={cambiar('password')} autoComplete="new-password" />
        </label>
        <div className="pnl-acciones pnl-ancho">
          <button type="button" className="pnl-boton" onClick={onCerrar}>
            Cerrar
          </button>
          <button type="submit" className="pnl-boton pnl-boton-principal" disabled={!valido || guardando}>
            {guardando ? 'Creando...' : 'Crear negocio'}
          </button>
        </div>
      </form>
    </div>
  );
}

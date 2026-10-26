import { useEffect, useState } from 'react';
import { KeyRound } from 'lucide-react';
import { panelApi } from '../../api/panel';
import { confirmar } from '../../utils/confirmar';

/**
 * Usuarios de un negocio: para cuando el dueño (o quien sea) olvidó su
 * contraseña y te escribe. Le pones una nueva, se la pasas y sus sesiones
 * abiertas se cierran. Ver handlers/cuenta.rs y handlers/panel.rs.
 */
export default function PanelUsuarios({ negocio }) {
  const [usuarios, setUsuarios] = useState(null);
  const [error, setError] = useState(null);
  const [editando, setEditando] = useState(null); // id del usuario
  const [clave, setClave] = useState('');
  const [ocupado, setOcupado] = useState(false);
  const [aviso, setAviso] = useState(null); // { tipo: 'ok'|'error', texto }

  useEffect(() => {
    let vigente = true;
    panelApi
      .usuarios(negocio.id)
      .then((lista) => vigente && setUsuarios(lista))
      .catch((e) => vigente && setError(e.message));
    return () => {
      vigente = false;
    };
  }, [negocio.id]);

  const abrir = (u) => {
    setAviso(null);
    setClave('');
    setEditando(u.id);
  };

  const guardar = async (u) => {
    if (clave.length < 8) {
      setAviso({ tipo: 'error', texto: 'La contraseña debe tener al menos 8 caracteres.' });
      return;
    }
    const ok = await confirmar({
      titulo: `¿Restablecer la contraseña de "${u.username}"?`,
      mensaje: 'Se cierra su sesión en todos sus dispositivos y desde ahora entra con la contraseña nueva.',
      detalle: { etiqueta: negocio.nombre, valor: u.nombre_completo },
      textoConfirmar: 'Restablecer',
      tipo: 'normal',
      icono: 'usuario',
    });
    if (!ok) return;
    setOcupado(true);
    setAviso(null);
    try {
      const r = await panelApi.restablecerClave(negocio.id, u.id, clave);
      setAviso({ tipo: 'ok', texto: `${u.username}: ${r.mensaje}` });
      setEditando(null);
      setClave('');
    } catch (e) {
      setAviso({ tipo: 'error', texto: e.message });
    } finally {
      setOcupado(false);
    }
  };

  if (error) return <p className="pnl-error">{error}</p>;
  if (!usuarios) return <p className="pnl-tenue">Cargando usuarios...</p>;

  return (
    <div className="pnl-caja">
      <h2>Usuarios del negocio</h2>
      <p className="pnl-nota">
        Si alguien olvidó su contraseña, ponle una nueva aquí y pásasela. Los trabajadores también pueden pedírsela al
        administrador del negocio (Configuración &gt; Usuarios).
      </p>

      {aviso && <p className={`pnl-aviso pnl-aviso-${aviso.tipo}`} style={{ marginTop: 12 }}>{aviso.texto}</p>}

      <div className="pnl-usuarios">
        {usuarios.map((u) => (
          <div key={u.id} className="pnl-usuario-fila">
            <div className="pnl-usuario-datos">
              <strong>{u.username}</strong>
              <span className="pnl-tenue">
                {u.nombre_completo} · {u.rol === 'INVENTARIO' ? 'ALMACÉN' : u.rol || 'sin rol'}
                {!u.activo && ' · inactivo'}
              </span>
            </div>
            {editando === u.id ? (
              <div className="pnl-linea">
                <input
                  type="text"
                  autoComplete="off"
                  autoFocus
                  placeholder="Contraseña nueva (mín. 8)"
                  value={clave}
                  onChange={(e) => setClave(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && guardar(u)}
                />
                <button type="button" className="pnl-boton pnl-boton-principal" disabled={ocupado} onClick={() => guardar(u)}>
                  Guardar
                </button>
                <button type="button" className="pnl-boton pnl-boton-texto" disabled={ocupado} onClick={() => setEditando(null)}>
                  Cancelar
                </button>
              </div>
            ) : (
              <button type="button" className="pnl-boton" onClick={() => abrir(u)}>
                <KeyRound size={15} /> Restablecer contraseña
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

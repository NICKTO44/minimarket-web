import { useState } from 'react';
import { api } from '../api/api';
import { pantallasDelUsuario } from '../utils/menu';
import './PantallasUsuario.css';

/**
 * Ventana para que el administrador elija qué pantallas ve un usuario
 * (Configuración → Usuarios → Pantallas). Se guardan las que NO ve: una
 * pantalla de un módulo que se encienda después aparece sola. El servidor
 * también bloquea lo propio de cada pantalla oculta (logica/pantallas.rs).
 */
export default function PantallasUsuario({ usuario, restaurante, modulos, onCerrar, onGuardado }) {
  const opciones = pantallasDelUsuario(usuario, { restaurante, modulos });
  const [ocultas, setOcultas] = useState(() => (usuario.pantallas_ocultas || []).filter((id) => opciones.some((o) => o.id === id)));
  const [guardando, setGuardando] = useState(false);
  const [error, setError] = useState('');

  const posVisible = !ocultas.includes('POS');
  // La Caja va con el Punto de venta: para vender hay que abrir la caja.
  const visible = (id) => !ocultas.includes(id) || (id === 'CAJA' && posVisible);
  const alternar = (id) => {
    setError('');
    setOcultas((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]));
  };
  const cuantasVe = opciones.filter((o) => visible(o.id)).length;

  const guardar = async () => {
    if (cuantasVe === 0) {
      setError('Deja al menos una pantalla: si no, el usuario no podría usar el sistema.');
      return;
    }
    setGuardando(true);
    try {
      const guardadas = await api.usuarioPantallas(usuario.id, ocultas);
      onGuardado(guardadas);
    } catch (e) {
      setError(e.message);
    } finally {
      setGuardando(false);
    }
  };

  const grupos = [...new Set(opciones.map((o) => o.grupo))];
  return (
    <div className="cfg-modal-overlay" onClick={onCerrar}>
      <div className="cfg-modal pantallas-modal" onClick={(e) => e.stopPropagation()}>
        <h2>Pantallas de {usuario.nombre_completo}</h2>
        <p className="pantallas-nota">
          Marca lo que <strong>{usuario.username}</strong> puede usar. Lo que desmarques no le aparece en el menú y el
          sistema no le deja usarlo. El cambio vale desde la próxima vez que abra el sistema.
        </p>
        {grupos.map((g) => (
          <fieldset key={g} className="pantallas-grupo">
            {g && <legend>{g}</legend>}
            {opciones
              .filter((o) => o.grupo === g)
              .map((o) => {
                const amarrada = o.id === 'CAJA' && posVisible;
                return (
                  <label key={o.id} className={`pantallas-opcion${amarrada ? ' pantallas-amarrada' : ''}`}>
                    <input type="checkbox" checked={visible(o.id)} disabled={amarrada} onChange={() => alternar(o.id)} />
                    <span>{o.label}</span>
                    {amarrada && <small>Va con el Punto de venta: necesita abrir su caja para vender.</small>}
                  </label>
                );
              })}
          </fieldset>
        ))}
        {error && <p className="pantallas-error">{error}</p>}
        <div className="cfg-modal-acciones">
          <button className="cfg-boton-cancelar" onClick={onCerrar}>
            Cancelar
          </button>
          <button className="cfg-boton-guardar-modal" onClick={guardar} disabled={guardando}>
            {guardando ? 'Guardando...' : 'Guardar'}
          </button>
        </div>
      </div>
    </div>
  );
}

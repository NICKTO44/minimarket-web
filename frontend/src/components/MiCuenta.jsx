import { useState } from 'react';
import { api, guardarTokenSesion } from '../api/api';
import './MiCuenta.css';

// Pestaña "Mi contraseña" de Configuración: el administrador cambia su propia
// contraseña o cierra su sesión en los demás dispositivos. El servidor
// entrega un token nuevo para este dispositivo, así que aquí no hay que
// volver a entrar; en los demás sí. (Las claves de los trabajadores se
// restablecen en la pestaña Usuarios.)

const VACIO = { actual: '', nueva: '', repetir: '' };

export default function MiCuenta({ onMensaje }) {
  const [form, setForm] = useState(VACIO);
  const [guardando, setGuardando] = useState(false);

  const cambiar = async (e) => {
    e.preventDefault();
    onMensaje(null);
    const error = (texto) => onMensaje({ tipo: 'error', texto });
    if (!form.actual) return error('Escribe tu contraseña actual.');
    if (form.nueva.length < 8) return error('La contraseña nueva debe tener al menos 8 caracteres.');
    if (form.nueva !== form.repetir) return error('La contraseña nueva no coincide al repetirla.');
    if (form.nueva === form.actual) return error('La contraseña nueva debe ser distinta de la actual.');
    setGuardando(true);
    try {
      const r = await api.cambiarMiClave(form.actual, form.nueva);
      guardarTokenSesion(r?.token);
      setForm(VACIO);
      onMensaje({ tipo: 'exito', texto: r?.mensaje || 'Contraseña cambiada.' });
    } catch (err) {
      error(err.message);
    } finally {
      setGuardando(false);
    }
  };

  const cerrarOtras = async () => {
    onMensaje(null);
    setGuardando(true);
    try {
      const r = await api.cerrarOtrasSesiones();
      guardarTokenSesion(r?.token);
      onMensaje({ tipo: 'exito', texto: r?.mensaje || 'Se cerró tu sesión en los demás dispositivos.' });
    } catch (err) {
      onMensaje({ tipo: 'error', texto: err.message });
    } finally {
      setGuardando(false);
    }
  };

  const campo = (clave, etiqueta, autoComplete) => (
    <div className="cfg-campo">
      <label>{etiqueta}</label>
      <input
        type="password"
        autoComplete={autoComplete}
        value={form[clave]}
        onChange={(e) => setForm({ ...form, [clave]: e.target.value })}
      />
    </div>
  );

  return (
    <div className="micuenta">
      <form className="cfg-card" onSubmit={cambiar}>
        <h3 className="cfg-subtitulo-seccion">Cambiar mi contraseña</h3>
        <p className="cfg-nota-moneda micuenta-nota">
          Al cambiarla se cierra tu sesión en los demás dispositivos; en este sigues conectado.
        </p>
        {campo('actual', 'Contraseña actual', 'current-password')}
        {campo('nueva', 'Contraseña nueva (mínimo 8 caracteres)', 'new-password')}
        {campo('repetir', 'Repite la contraseña nueva', 'new-password')}
        <button type="submit" className="cfg-boton-guardar" disabled={guardando}>
          {guardando ? 'Guardando...' : 'Cambiar contraseña'}
        </button>
      </form>

      <div className="cfg-card">
        <h3 className="cfg-subtitulo-seccion">Sesiones abiertas</h3>
        <p className="cfg-nota-moneda micuenta-nota">
          ¿Dejaste tu sesión abierta en otra computadora o celular? Ciérrala en todos los demás dispositivos sin cambiar
          tu contraseña.
        </p>
        <button type="button" className="micuenta-boton-secundario" onClick={cerrarOtras} disabled={guardando}>
          Cerrar sesión en los demás dispositivos
        </button>
      </div>
    </div>
  );
}

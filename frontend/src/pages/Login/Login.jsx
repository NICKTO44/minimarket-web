import { useState } from 'react';
import { api } from '../../api/api';
import './Login.css';

export default function Login({ tiendaRecordada, onLoginExitoso, onIrARegistro, onOlvidarTienda }) {
  const [usuario, setUsuario] = useState('');
  const [password, setPassword] = useState('');
  const [cargando, setCargando] = useState(false);
  const [mensaje, setMensaje] = useState('');

  const handleSubmit = async (e) => {
    e.preventDefault();
    setCargando(true);
    setMensaje('');
    try {
      const data = await api.login(usuario, password, tiendaRecordada?.identificador);
      if (data?.ok && data.token) {
        onLoginExitoso(data);
      }
    } catch (err) {
      setMensaje(err.message || 'Usuario o contraseña incorrectos');
    } finally {
      setCargando(false);
    }
  };

  return (
    <div className="login-container">
      <form className="login-form" onSubmit={handleSubmit}>
        {tiendaRecordada ? (
          <>
            <h1>Bienvenido</h1>
            <p className="login-negocio">{tiendaRecordada.nombre_negocio}</p>
          </>
        ) : (
          <h1>Minimarket POS</h1>
        )}

        <input
          type="text"
          placeholder="Usuario"
          value={usuario}
          onChange={(e) => setUsuario(e.target.value)}
          autoFocus
        />
        <input
          type="password"
          placeholder="Contraseña"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <button type="submit" disabled={cargando}>
          {cargando ? 'Ingresando...' : 'Ingresar'}
        </button>
        {mensaje && <p className="login-mensaje-error">{mensaje}</p>}

        {tiendaRecordada ? (
          <button type="button" className="login-enlace" onClick={onOlvidarTienda}>
            ¿No es tu negocio? Cambiar
          </button>
        ) : (
          <button type="button" className="login-enlace" onClick={onIrARegistro}>
            ¿Tu negocio no está registrado todavía? Regístrate
          </button>
        )}
      </form>
    </div>
  );
}
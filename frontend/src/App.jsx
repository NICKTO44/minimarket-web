import { useState, useEffect } from 'react';
import { api } from './api/api';
import POS from './pages/POS/POS';
import Caja from './pages/Caja/Caja';
import Sidebar from './components/Sidebar';
import './App.css';
import Inventario from './pages/Inventario/Inventario';
import StockLotes from './pages/StockLotes/StockLotes';
import Proveedores from './pages/Proveedores/Proveedores';
import Devoluciones from './pages/Devoluciones/Devoluciones';
import Clientes from './pages/Clientes/Clientes';
import Comprobantes from './pages/Comprobantes/Comprobantes';
import Reportes from './pages/Reportes/Reportes';
import Configuracion from './pages/Configuracion/Configuracion';
import Resumen from './pages/Resumen/Resumen';
import HistorialCaja from './pages/Caja/HistorialCaja';

const STORAGE_KEY = 'minimarket_sesion';

function App() {
  const [usuario, setUsuario] = useState('');
  const [password, setPassword] = useState('');
  const [logueado, setLogueado] = useState(false);
  const [usuarioActual, setUsuarioActual] = useState(null);
  const [mensaje, setMensaje] = useState('');
  const [cargando, setCargando] = useState(false);
  const [pantalla, setPantalla] = useState('RESUMEN');
  const [configuracionTienda, setConfiguracionTienda] = useState(null);
  const [verificandoSesion, setVerificandoSesion] = useState(true);

  useEffect(() => {
    const guardado = localStorage.getItem(STORAGE_KEY);
    if (guardado) {
      try {
        const sesion = JSON.parse(guardado);
        if (sesion?.token && sesion?.usuario) {
          setUsuarioActual(sesion.usuario);
          setLogueado(true);
          api.configuracionObtener().then(setConfiguracionTienda).catch(() => {});
        }
      } catch {
        localStorage.removeItem(STORAGE_KEY);
      }
    }
    setVerificandoSesion(false);
  }, []);

  const handleLogin = async (e) => {
    e.preventDefault();
    setCargando(true);
    setMensaje('');
    try {
      const data = await api.login(usuario, password);
      if (data?.ok && data.token) {
        const sesionUsuario = {
          id: data.usuario.id,
          nombre: data.usuario.nombre_completo,
          username: data.usuario.username,
          rol_id: data.usuario.rol_id,
        };
        setUsuarioActual(sesionUsuario);
        setLogueado(true);
        localStorage.setItem(STORAGE_KEY, JSON.stringify({ token: data.token, usuario: sesionUsuario }));
        api.configuracionObtener().then(setConfiguracionTienda).catch(() => {});
      }
    } catch (err) {
      setMensaje(err.message || 'Usuario o contraseña incorrectos');
    } finally {
      setCargando(false);
    }
  };

  const handleLogout = () => {
    setLogueado(false);
    setUsuarioActual(null);
    setUsuario('');
    setPassword('');
    setPantalla('RESUMEN');
    setConfiguracionTienda(null);
    localStorage.removeItem(STORAGE_KEY);
  };

  if (verificandoSesion) {
    return null;
  }

  if (!logueado) {
    return (
      <div className="login-container">
        <form className="login-form" onSubmit={handleLogin}>
          <h1>Minimarket POS</h1>
          <input
            type="text"
            placeholder="Usuario"
            value={usuario}
            onChange={(e) => setUsuario(e.target.value)}
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
          {mensaje && <p className="mensaje-error">{mensaje}</p>}
        </form>
      </div>
    );
  }

  const nombreTienda = configuracionTienda?.nombre_tienda || 'Mi Minimarket';

  return (
    <div className="app-shell">
      <Sidebar
        pantalla={pantalla}
        onCambiarPantalla={setPantalla}
        usuario={usuarioActual}
        onLogout={handleLogout}
        nombreTienda={nombreTienda}
        ruc={configuracionTienda?.ruc}
      />
      <div className="app-contenido">
        {pantalla === 'POS' && <POS usuario={usuarioActual} nombreTienda={nombreTienda} />}
        {pantalla === 'RESUMEN' && <Resumen onIrA={setPantalla} />}
        {pantalla === 'CAJA' && <Caja usuario={usuarioActual} />}
        {pantalla === 'HISTORIAL_CAJA' && <HistorialCaja />}
        {pantalla === 'PRODUCTOS' && <Inventario />}
        {pantalla === 'STOCK' && <StockLotes />}
        {pantalla === 'PROVEEDORES' && <Proveedores />}
        {pantalla === 'DEVOLUCIONES' && <Devoluciones usuario={usuarioActual} />}
        {pantalla === 'CLIENTES' && <Clientes />}
        {pantalla === 'COMPROBANTES' && <Comprobantes usuario={usuarioActual} nombreTienda={nombreTienda} />}
        {pantalla === 'REPORTES' && <Reportes />}
        {pantalla === 'CONFIGURACION' && <Configuracion />}
      </div>
    </div>
  );
}

export default App;
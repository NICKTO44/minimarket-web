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
import Login from './pages/Login/Login';
import Registro from './pages/Registro/Registro';

const STORAGE_KEY = 'minimarket_sesion';
// Aparte de la sesión: qué negocio pertenece a este dispositivo/navegador.
// A propósito NO se borra al cerrar sesión — así el próximo login (mismo
// admin u otro cajero) no tiene que volver a escribir el negocio.
const TIENDA_STORAGE_KEY = 'minimarket_tienda';

function App() {
  const [logueado, setLogueado] = useState(false);
  const [usuarioActual, setUsuarioActual] = useState(null);
  const [pantalla, setPantalla] = useState('RESUMEN');
  const [configuracionTienda, setConfiguracionTienda] = useState(null);
  const [verificandoSesion, setVerificandoSesion] = useState(true);
  const [vistaAuth, setVistaAuth] = useState('login'); // 'login' | 'registro'
  const [tiendaRecordada, setTiendaRecordada] = useState(null);

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

    const tiendaGuardada = localStorage.getItem(TIENDA_STORAGE_KEY);
    if (tiendaGuardada) {
      try {
        setTiendaRecordada(JSON.parse(tiendaGuardada));
      } catch {
        localStorage.removeItem(TIENDA_STORAGE_KEY);
      }
    }

    setVerificandoSesion(false);
  }, []);

  // Usado tanto por Login como por Registro (Registro hace login solo
  // apenas crea el negocio) — así toda la lógica de guardar sesión vive
  // en un solo lugar.
  const handleLoginExitoso = (data) => {
    const sesionUsuario = {
      id: data.usuario.id,
      nombre: data.usuario.nombre_completo,
      username: data.usuario.username,
      rol_id: data.usuario.rol_id,
    };
    setUsuarioActual(sesionUsuario);
    setLogueado(true);
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ token: data.token, usuario: sesionUsuario }));

    if (data.tienda) {
      localStorage.setItem(TIENDA_STORAGE_KEY, JSON.stringify(data.tienda));
      setTiendaRecordada(data.tienda);
    }

    api.configuracionObtener().then(setConfiguracionTienda).catch(() => {});
  };

  const handleLogout = () => {
    setLogueado(false);
    setUsuarioActual(null);
    setPantalla('RESUMEN');
    setConfiguracionTienda(null);
    localStorage.removeItem(STORAGE_KEY);
    // TIENDA_STORAGE_KEY se queda — ver nota arriba.
  };

  const handleOlvidarTienda = () => {
    localStorage.removeItem(TIENDA_STORAGE_KEY);
    setTiendaRecordada(null);
  };

  if (verificandoSesion) {
    return null;
  }

  if (!logueado) {
    if (vistaAuth === 'registro') {
      return <Registro onRegistroExitoso={handleLoginExitoso} onIrALogin={() => setVistaAuth('login')} />;
    }
    return (
      <Login
        tiendaRecordada={tiendaRecordada}
        onLoginExitoso={handleLoginExitoso}
        onIrARegistro={() => setVistaAuth('registro')}
        onOlvidarTienda={handleOlvidarTienda}
      />
    );
  }

  const nombreTienda = configuracionTienda?.nombre_tienda || tiendaRecordada?.nombre_negocio || 'Mi Minimarket';

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
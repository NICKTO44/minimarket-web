import { useState, useEffect } from 'react';
import { api } from './api/api';
import POS from './pages/POS/POS';
import PosBloqueado from './components/PosBloqueado';
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
import Suscripcion from './pages/Suscripcion/Suscripcion';
import Resumen from './pages/Resumen/Resumen';
import HistorialCaja from './pages/Caja/HistorialCaja';
import Login from './pages/Login/Login';
import Registro from './pages/Registro/Registro';
import BoletaPublica from './pages/BoletaPublica/BoletaPublica';

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

  // Modo lectura por suscripción no al día — viene del login (backend
  // ya lo calcula y lo aplica de verdad en cada escritura vía el
  // middleware; esto es solo para mostrarlo de entrada en el frontend
  // sin esperar a que una acción falle con 402).
  const [modoLectura, setModoLectura] = useState(false);
  const [avisoSuscripcion, setAvisoSuscripcion] = useState(null);

  // Estado completo de la suscripción (días restantes, fecha de
  // vencimiento, etc.) — se usa en el sidebar (punto de aviso) y en la
  // página dedicada de Suscripción.
  const [estadoSuscripcion, setEstadoSuscripcion] = useState(null);

  const cargarEstadoSuscripcion = () => {
    api.suscripcionEstado().then(setEstadoSuscripcion).catch(() => {});
  };

  useEffect(() => {
    const guardado = localStorage.getItem(STORAGE_KEY);
    if (guardado) {
      try {
        const sesion = JSON.parse(guardado);
        if (sesion?.token && sesion?.usuario) {
          setUsuarioActual(sesion.usuario);
          setLogueado(true);
          setModoLectura(!!sesion.modoLectura);
          setAvisoSuscripcion(sesion.aviso || null);
          api.configuracionObtener().then(setConfiguracionTienda).catch(() => {});
          cargarEstadoSuscripcion();
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    setModoLectura(!!data.modo_lectura);
    setAvisoSuscripcion(data.aviso || null);
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        token: data.token,
        usuario: sesionUsuario,
        modoLectura: !!data.modo_lectura,
        aviso: data.aviso || null,
      })
    );

    if (data.tienda) {
      localStorage.setItem(TIENDA_STORAGE_KEY, JSON.stringify(data.tienda));
      setTiendaRecordada(data.tienda);
    }

    api.configuracionObtener().then(setConfiguracionTienda).catch(() => {});
    cargarEstadoSuscripcion();
  };

  const handleLogout = () => {
    setLogueado(false);
    setUsuarioActual(null);
    setPantalla('RESUMEN');
    setConfiguracionTienda(null);
    setModoLectura(false);
    setAvisoSuscripcion(null);
    setEstadoSuscripcion(null);
    localStorage.removeItem(STORAGE_KEY);
    // TIENDA_STORAGE_KEY se queda — ver nota arriba.
  };

  const handleOlvidarTienda = () => {
    localStorage.removeItem(TIENDA_STORAGE_KEY);
    setTiendaRecordada(null);
  };

  // Se llama justo después de canjear un código de activación con
  // éxito. Actualiza el estado en memoria Y lo que quedó guardado en
  // localStorage — así, aunque recargues la página sin volver a
  // loguearte, el sistema sigue sabiendo que ya no está en modo lectura.
  // También recarga el estado completo (días restantes, fecha) para que
  // el sidebar y la página de Suscripción se vean actualizados al toque.
  const handleSuscripcionActivada = () => {
    setModoLectura(false);
    setAvisoSuscripcion(null);
    const guardado = localStorage.getItem(STORAGE_KEY);
    if (guardado) {
      try {
        const sesion = JSON.parse(guardado);
        localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...sesion, modoLectura: false, aviso: null }));
      } catch {
        // si el guardado estaba corrupto, no hay nada que actualizar
      }
    }
    cargarEstadoSuscripcion();
  };

  // Ruta pública, sin login — un cliente final llega aquí desde un link
  // de WhatsApp, nunca inició sesión en el sistema. Se revisa después de
  // declarar todos los hooks (regla de React), pero ANTES de cualquier
  // lógica de sesión/login.
  const matchBoletaPublica = window.location.pathname.match(/^\/boleta\/([^/]+)\/(\d+)$/);
  if (matchBoletaPublica) {
    const [, identificadorUrl, comprobanteIdUrl] = matchBoletaPublica;
    return <BoletaPublica identificador={identificadorUrl} comprobanteId={comprobanteIdUrl} />;
  }

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
        diasRestantesSuscripcion={estadoSuscripcion?.dias_restantes ?? null}
      />
      <div className="app-contenido">
        {pantalla === 'POS' &&
          (modoLectura ? (
            <PosBloqueado aviso={avisoSuscripcion} />
          ) : (
            <POS
              usuario={usuarioActual}
              nombreTienda={nombreTienda}
              direccion={configuracionTienda?.direccion}
              telefono={configuracionTienda?.telefono}
              ruc={configuracionTienda?.ruc}
              identificadorNegocio={tiendaRecordada?.identificador}
            />
          ))}
        {pantalla === 'RESUMEN' && <Resumen onIrA={setPantalla} />}
        {pantalla === 'CAJA' && <Caja usuario={usuarioActual} />}
        {pantalla === 'HISTORIAL_CAJA' && <HistorialCaja />}
        {pantalla === 'PRODUCTOS' && <Inventario />}
        {pantalla === 'STOCK' && <StockLotes />}
        {pantalla === 'PROVEEDORES' && <Proveedores />}
        {pantalla === 'DEVOLUCIONES' && <Devoluciones usuario={usuarioActual} />}
        {pantalla === 'CLIENTES' && <Clientes />}
        {pantalla === 'COMPROBANTES' && (
          <Comprobantes
            usuario={usuarioActual}
            nombreTienda={nombreTienda}
            direccion={configuracionTienda?.direccion}
            telefono={configuracionTienda?.telefono}
            ruc={configuracionTienda?.ruc}
            identificadorNegocio={tiendaRecordada?.identificador}
          />
        )}
        {pantalla === 'REPORTES' && <Reportes />}
        {pantalla === 'SUSCRIPCION' && (
          <Suscripcion
            estadoSuscripcion={estadoSuscripcion}
            onRecargar={cargarEstadoSuscripcion}
            onSuscripcionActivada={handleSuscripcionActivada}
          />
        )}
        {pantalla === 'CONFIGURACION' && <Configuracion />}
      </div>
    </div>
  );
}

export default App;
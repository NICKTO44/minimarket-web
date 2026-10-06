import { useState, useEffect } from 'react';
import { api } from './api/api';
import { aplicarTema, limpiarTema } from './utils/tema';
import POS from './pages/POS/POS';
import PosBloqueado from './components/PosBloqueado';
import Caja from './pages/Caja/Caja';
import Sidebar from './components/Sidebar';
import NavegacionMovil from './components/NavegacionMovil';
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
import Mesas from './pages/Mesas/Mesas';
import Preparacion from './pages/Preparacion/Preparacion';
import CartaDia from './pages/CartaDia/CartaDia';
import Cotizaciones from './pages/Cotizaciones/Cotizaciones';
import Creditos from './pages/Creditos/Creditos';
import Guias from './pages/Guias/Guias';
import AvisosListos from './components/AvisosListos';
import {
  esAlmacen,
  esCajero,
  esMesero,
  esPreparacion,
  PANTALLAS_ALMACEN,
  PANTALLAS_MESERO,
  PANTALLAS_PREPARACION,
} from './utils/menu';
import {
  datosNegocio,
  MODULO_CAMBIOS,
  MODULO_COTIZACIONES,
  MODULO_CREDITO,
  MODULO_GUIAS,
  MODULO_MEDIDAS,
  MODULO_MESAS,
  MODULO_SERVICIOS,
  MODULO_GANANCIAS, MODULO_PRECIO_VENTA, MODULO_VARIANTES,
  rubroDe,
} from './utils/rubros';

const STORAGE_KEY = 'minimarket_sesion';
const TIENDA_STORAGE_KEY = 'minimarket_tienda';

// Pantallas que solo existen con su módulo encendido.
const MODULO_DE_PANTALLA = {
  COTIZACIONES: MODULO_COTIZACIONES,
  CREDITOS: MODULO_CREDITO,
  GUIAS: MODULO_GUIAS,
};

function App() {
  const [logueado, setLogueado] = useState(false);
  const [usuarioActual, setUsuarioActual] = useState(null);
  const [pantalla, setPantalla] = useState('RESUMEN');
  // Reportes → Ganancias pide abrir Productos ya filtrado por "sin precio de compra".
  const [productosSinPrecio, setProductosSinPrecio] = useState(false);
  const [configuracionTienda, setConfiguracionTienda] = useState(null);
  const [verificandoSesion, setVerificandoSesion] = useState(true);
  const [vistaAuth, setVistaAuth] = useState('login');
  const [tiendaRecordada, setTiendaRecordada] = useState(null);

  const [modoLectura, setModoLectura] = useState(false);
  const [avisoSuscripcion, setAvisoSuscripcion] = useState(null);
  const [estadoSuscripcion, setEstadoSuscripcion] = useState(null);
  // Cambia cuando el negocio sube un logo nuevo, para que el navegador
  // no siga mostrando la imagen anterior guardada en caché.
  const [versionLogo, setVersionLogo] = useState(() => Date.now());
  // Cafetería / Restaurante: pedido que se está cobrando en el POS, y el
  // pedido que se debe abrir en Mesas (al cancelar un cobro o desde un
  // aviso de "listo"). n cambia en cada pedido de apertura.
  const [pedidoACobrar, setPedidoACobrar] = useState(null);
  // Cotización abierta desde su pantalla para venderla en el punto de venta.
  const [cotizacionACargar, setCotizacionACargar] = useState(null);
  // Cambio de prenda en curso: lo que el cliente devuelve; lo que se lleva
  // se elige en el punto de venta.
  const [cambioEnCurso, setCambioEnCurso] = useState(null);
  const [abrirPedido, setAbrirPedido] = useState(null);
  const [pedidosListos, setPedidosListos] = useState(0);

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
          try {
            const tienda = JSON.parse(localStorage.getItem(TIENDA_STORAGE_KEY) || 'null');
            if (esAlmacen(sesion.usuario)) {
              setPantalla('PRODUCTOS');
            } else if (tienda?.modo_negocio === 'RESTAURANTE') {
              setPantalla(esPreparacion(sesion.usuario) ? 'PREPARACION' : 'MESAS');
            }
          } catch {
            // sin negocio recordado: se queda en Resumen
          }
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

  const handleLoginExitoso = (data) => {
    const sesionUsuario = {
      id: data.usuario.id,
      nombre: data.usuario.nombre_completo,
      username: data.usuario.username,
      rol_id: data.usuario.rol_id,
      rol_nombre: data.usuario.rol_nombre || null,
    };
    // En una cafetería/restaurante se entra directo al mapa de mesas.
    // Almacén entra a Productos, que es lo suyo.
    setPantalla(
      esAlmacen(sesionUsuario)
        ? 'PRODUCTOS'
        : data.tienda?.modo_negocio !== 'RESTAURANTE'
          ? 'RESUMEN'
          : sesionUsuario.rol_nombre === 'PREPARACION'
            ? 'PREPARACION'
            : 'MESAS'
    );
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
    } else {
      // Sin datos del negocio (p. ej. recién registrado): no se reutiliza
      // el negocio recordado de otra cuenta en este navegador, para que
      // su logo o color no aparezcan ni por un instante.
      localStorage.removeItem(TIENDA_STORAGE_KEY);
      setTiendaRecordada(null);
    }

    api.configuracionObtener().then(setConfiguracionTienda).catch(() => {});
    cargarEstadoSuscripcion();
  };

  // Paso 1 del login en dos pasos. Se llama cuando el backend ya
  // identificó a qué negocio pertenece el usuario escrito (con su logo
  // y color), pero TODAVÍA no hay sesión. Solo recuerda el negocio.
  const handleTiendaIdentificada = (tienda) => {
    localStorage.setItem(TIENDA_STORAGE_KEY, JSON.stringify(tienda));
    setTiendaRecordada(tienda);
  };
    // Se llama desde Configuración cuando el dueño cambia el logo o el
  // color de acento -- actualiza tiendaRecordada al toque, sin esperar
  // a que alguien vuelva a loguearse para que el cambio se vea.
  const handleIdentidadActualizada = (cambios) => {
    // El tema y el logo del sidebar salen de configuracionTienda, así que
    // se actualiza también para que el cambio se vea en todo el sistema.
    setConfiguracionTienda((actual) =>
      actual ? { ...actual, logo_path: cambios.logo_url, color_acento: cambios.color_acento } : actual
    );
    setVersionLogo(Date.now());
    setTiendaRecordada((actual) => {
      if (!actual) return actual;
      const actualizada = { ...actual, ...cambios };
      localStorage.setItem(TIENDA_STORAGE_KEY, JSON.stringify(actualizada));
      return actualizada;
    });
  };

  const handleLogout = () => {
    setLogueado(false);
    setUsuarioActual(null);
    setPantalla('RESUMEN');
    setConfiguracionTienda(null);
    setPedidoACobrar(null);
    setAbrirPedido(null);
    setPedidosListos(0);
    setModoLectura(false);
    setAvisoSuscripcion(null);
    setEstadoSuscripcion(null);
    localStorage.removeItem(STORAGE_KEY);
  };

  const handleOlvidarTienda = () => {
    localStorage.removeItem(TIENDA_STORAGE_KEY);
    setTiendaRecordada(null);
  };

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

  // Identidad visual del negocio con sesión activa. Mientras llega su
  // configuración del servidor se usa la del negocio recordado (es el
  // mismo, se guardó en este login); después, siempre la del servidor.
  const identidadNegocio = configuracionTienda
    ? { color: configuracionTienda.color_acento, logo: configuracionTienda.logo_path }
    : { color: tiendaRecordada?.color_acento, logo: tiendaRecordada?.logo_url };

  useEffect(() => {
    if (logueado) {
      aplicarTema(identidadNegocio.color);
    } else {
      limpiarTema();
    }
  }, [logueado, identidadNegocio.color]);

  // Título de la pestaña: "Monspeet POS" fuera de sesión y, dentro, el
  // nombre del negocio que inició sesión ("Bodega Juan · Monspeet POS"),
  // para que cada cliente vea su propio negocio en el navegador.
  const nombreNegocioTitulo = configuracionTienda?.nombre_tienda || tiendaRecordada?.nombre_negocio;
  useEffect(() => {
    document.title = logueado && nombreNegocioTitulo ? `${nombreNegocioTitulo} · Monspeet POS` : 'Monspeet POS';
  }, [logueado, nombreNegocioTitulo]);

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
        onTiendaIdentificada={handleTiendaIdentificada}
        onIrARegistro={() => setVistaAuth('registro')}
        onOlvidarTienda={handleOlvidarTienda}
      />
    );
  }

  const nombreTienda = configuracionTienda?.nombre_tienda || tiendaRecordada?.nombre_negocio || 'Mi Minimarket';

  // ¿El negocio atiende en mesas? Mientras llega la configuración del
  // servidor se usa lo que dijo el login.
  // Rubro y módulos del negocio (núcleo universal + módulos encendidos).
  const negocio = datosNegocio(configuracionTienda ?? tiendaRecordada);
  const restaurante = negocio.modulos.includes(MODULO_MESAS);
  // Nombres de pantalla propios del rubro ("Carta" en vez de "Productos").
  const etiquetas = rubroDe(negocio.rubro).etiquetas;
  // El mesero solo toma pedidos (Mesas) y barra/cocina solo ve
  // Preparación; ambos pueden además armar la Carta de hoy.
  const soloMesas = restaurante && esMesero(usuarioActual);
  const soloPreparacion = restaurante && esPreparacion(usuarioActual);
  // Almacén (en cualquier rubro): productos, stock, proveedores y reportes.
  const soloAlmacen = esAlmacen(usuarioActual);
  const pantallaVisible = soloAlmacen
    ? PANTALLAS_ALMACEN.includes(pantalla)
      ? pantalla
      : 'PRODUCTOS'
    : soloPreparacion
    ? PANTALLAS_PREPARACION.includes(pantalla)
      ? pantalla
      : 'PREPARACION'
    : soloMesas
      ? PANTALLAS_MESERO.includes(pantalla)
        ? pantalla
        : 'MESAS'
      : ['MESAS', 'PREPARACION', 'CARTA'].includes(pantalla) && !restaurante
        ? 'RESUMEN'
        : MODULO_DE_PANTALLA[pantalla] && !negocio.modulos.includes(MODULO_DE_PANTALLA[pantalla])
          ? 'RESUMEN'
        : pantalla === 'CARTA' && esCajero(usuarioActual)
          ? 'MESAS'
          : pantalla;

  const irAPedido = (id) => {
    setAbrirPedido((actual) => ({ id, n: (actual?.n || 0) + 1 }));
    setPantalla('MESAS');
  };

  const handleCobrarPedido = (pedido) => {
    setPedidoACobrar(pedido);
    setPantalla('POS');
  };

  const handleCancelarCobroPedido = () => {
    const id = pedidoACobrar?.id;
    setPedidoACobrar(null);
    if (id) irAPedido(id);
    else setPantalla('MESAS');
  };

  // Configuración cambió el rubro o los módulos: { rubro, modulos, modo_negocio }.
  const handleNegocioCambiado = (cambio) => {
    setConfiguracionTienda((actual) => (actual ? { ...actual, ...cambio } : actual));
    setTiendaRecordada((actual) => {
      if (!actual) return actual;
      const actualizada = { ...actual, ...cambio };
      localStorage.setItem(TIENDA_STORAGE_KEY, JSON.stringify(actualizada));
      return actualizada;
    });
  };

  return (
    <div className="app-shell">
      <Sidebar
        pantalla={pantallaVisible}
        restaurante={restaurante}
        etiquetas={etiquetas}
        modulos={negocio.modulos}
        insignias={{ MESAS: pedidosListos }}
        onCambiarPantalla={setPantalla}
        usuario={usuarioActual}
        onLogout={handleLogout}
        nombreTienda={nombreTienda}
        ruc={configuracionTienda?.ruc}
        diasRestantesSuscripcion={estadoSuscripcion?.dias_restantes ?? null}
        logoUrl={identidadNegocio.logo}
        versionLogo={versionLogo}
      />
      <div className="app-contenido">
        {pantallaVisible === 'MESAS' && (
          <Mesas
            usuario={usuarioActual}
            nombreTienda={nombreTienda}
            onCobrar={handleCobrarPedido}
            abrirPedido={abrirPedido}
            onAbrirPedidoUsado={() => setAbrirPedido(null)}
            onIrACarta={esCajero(usuarioActual) ? null : () => setPantalla('CARTA')}
          />
        )}
        {pantallaVisible === 'PREPARACION' && restaurante && <Preparacion />}
        {pantallaVisible === 'CARTA' && restaurante && <CartaDia />}
        {pantallaVisible === 'POS' &&
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
              medidas={negocio.modulos.includes(MODULO_MEDIDAS)}
              cotizaciones={negocio.modulos.includes(MODULO_COTIZACIONES)}
              credito={negocio.modulos.includes(MODULO_CREDITO)}
              cotizacionACargar={cotizacionACargar}
              onCotizacionUsada={() => setCotizacionACargar(null)}
              variantes={negocio.modulos.includes(MODULO_VARIANTES)}
              precioEditable={negocio.modulos.includes(MODULO_PRECIO_VENTA)}
              cambios={negocio.modulos.includes(MODULO_CAMBIOS)}
              cambioEnCurso={negocio.modulos.includes(MODULO_CAMBIOS) ? cambioEnCurso : null}
              onCambioTerminado={() => setCambioEnCurso(null)}
              pedidoACobrar={pedidoACobrar}
              onCancelarCobroPedido={handleCancelarCobroPedido}
              onPedidoCobrado={() => setPedidoACobrar(null)}
              onVolverAMesas={restaurante ? () => setPantalla('MESAS') : null}
            />
          ))}
        {pantallaVisible === 'RESUMEN' && <Resumen onIrA={setPantalla} />}
        {pantallaVisible === 'CAJA' && <Caja usuario={usuarioActual} />}
        {pantallaVisible === 'HISTORIAL_CAJA' && <HistorialCaja />}
        {pantallaVisible === 'PRODUCTOS' && (
          <Inventario
            servicios={negocio.modulos.includes(MODULO_SERVICIOS)}
            etiquetas={etiquetas}
            esAdmin={usuarioActual?.rol_id === 1}
            variantes={negocio.modulos.includes(MODULO_VARIANTES)}
            ganancias={negocio.modulos.includes(MODULO_GANANCIAS)}
            verSinPrecio={productosSinPrecio}
            onSinPrecioVisto={() => setProductosSinPrecio(false)}
          />
        )}
        {pantallaVisible === 'STOCK' && <StockLotes />}
        {pantallaVisible === 'PROVEEDORES' && <Proveedores />}
        {pantallaVisible === 'DEVOLUCIONES' && (
          <Devoluciones
            usuario={usuarioActual}
            cambios={negocio.modulos.includes(MODULO_CAMBIOS)}
            onIniciarCambio={(cambio) => {
              setPedidoACobrar(null);
              setCotizacionACargar(null);
              setCambioEnCurso(cambio);
              setPantalla('POS');
            }}
          />
        )}
        {pantallaVisible === 'CLIENTES' && <Clientes />}
        {pantallaVisible === 'COTIZACIONES' && (
          <Cotizaciones
            nombreTienda={nombreTienda}
            direccion={configuracionTienda?.direccion}
            telefono={configuracionTienda?.telefono}
            ruc={configuracionTienda?.ruc}
            onCargarEnVenta={(cotizacion) => {
              setPedidoACobrar(null);
              setCotizacionACargar(cotizacion);
              setPantalla('POS');
            }}
            onIrAVender={() => setPantalla('POS')}
          />
        )}
        {pantallaVisible === 'CREDITOS' && (
          <Creditos
            nombreTienda={nombreTienda}
            direccion={configuracionTienda?.direccion}
            telefono={configuracionTienda?.telefono}
            ruc={configuracionTienda?.ruc}
          />
        )}
        {pantallaVisible === 'GUIAS' && <Guias esAdmin={usuarioActual?.rol_id === 1} />}
        {pantallaVisible === 'COMPROBANTES' && (
          <Comprobantes
            usuario={usuarioActual}
            nombreTienda={nombreTienda}
            direccion={configuracionTienda?.direccion}
            telefono={configuracionTienda?.telefono}
            ruc={configuracionTienda?.ruc}
            identificadorNegocio={tiendaRecordada?.identificador}
          />
        )}
        {pantallaVisible === 'REPORTES' && (
          <Reportes
            ganancias={negocio.modulos.includes(MODULO_GANANCIAS) && usuarioActual?.rol_id === 1}
            onVerSinPrecio={() => {
              setProductosSinPrecio(true);
              setPantalla('PRODUCTOS');
            }}
          />
        )}
        {pantallaVisible === 'SUSCRIPCION' && usuarioActual?.rol_id === 1 && (
          <Suscripcion
            estadoSuscripcion={estadoSuscripcion}
            onRecargar={cargarEstadoSuscripcion}
            onSuscripcionActivada={handleSuscripcionActivada}
          />
        )}
        {pantallaVisible === 'CONFIGURACION' && usuarioActual?.rol_id === 1 && (
          <Configuracion
            onIdentidadActualizada={handleIdentidadActualizada}
            usuarioActualId={usuarioActual.id}
            onNegocioCambiado={handleNegocioCambiado}
          />
        )}
      </div>
      {/* Solo se ve en celular (<= 899px): barra superior + barra inferior */}
      {/* Avisos "pedido listo" para mozo y cajero, en cualquier pantalla */}
      {restaurante && !soloPreparacion && !soloAlmacen && (
        <AvisosListos usuario={usuarioActual} onAbrirPedido={irAPedido} onConteo={setPedidosListos} />
      )}
      <NavegacionMovil
        pantalla={pantallaVisible}
        restaurante={restaurante}
        etiquetas={etiquetas}
        modulos={negocio.modulos}
        insignias={{ MESAS: pedidosListos }}
        onCambiarPantalla={setPantalla}
        usuario={usuarioActual}
        onLogout={handleLogout}
        nombreTienda={nombreTienda}
        diasRestantesSuscripcion={estadoSuscripcion?.dias_restantes ?? null}
        logoUrl={identidadNegocio.logo}
        versionLogo={versionLogo}
      />
    </div>
  );
}

export default App;
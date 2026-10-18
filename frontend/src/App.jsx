import { useState, useEffect, useRef, lazy, Suspense } from 'react';
import { api } from './api/api';
import { aplicarTema, limpiarTema } from './utils/tema';
import POS from './pages/POS/POS';
import PosBloqueado from './components/PosBloqueado';
import Caja from './pages/Caja/Caja';
import Sidebar from './components/Sidebar';
import NavegacionMovil from './components/NavegacionMovil';
import './App.css';
import Inventario from './pages/Inventario/Inventario';
import LibroRecetas from './pages/Recetas/LibroRecetas';
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
import Gastos from './pages/Gastos/Gastos';
import Vehiculos from './pages/Vehiculos/Vehiculos';
import Ordenes from './pages/Ordenes/Ordenes';
import Paquetes from './pages/Paquetes/Paquetes';
import AvisosListos from './components/AvisosListos';
import DireccionNegocio from './components/DireccionNegocio';
import { irADireccion, leerDireccion } from './utils/direccion';
import { usarAppDelNegocio } from './utils/appInstalable';
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
  MODULO_GANANCIAS, MODULO_PRECIO_VENTA, MODULO_VARIANTES, MODULO_PRESENTACIONES, MODULO_PRECIO_MAYOR,
  MODULO_FICHA_MEDICAMENTO,
  MODULO_RECETAS,
  MODULO_ALTERNATIVAS,
  MODULO_FICHA_REPUESTO,
  MODULO_COMPATIBILIDAD,
  MODULO_VEHICULOS,
  MODULO_ORDENES_SERVICIO,
  MODULO_PAQUETES,
  rubroDe,
} from './utils/rubros';

// Panel del dueño de Monspeet (/panel): se carga solo si se abre esa dirección.
const Panel = lazy(() => import('./pages/Panel/Panel'));

const STORAGE_KEY = 'minimarket_sesion';
const TIENDA_STORAGE_KEY = 'minimarket_tienda';

// Pantallas que solo existen con su módulo encendido.
const MODULO_DE_PANTALLA = {
  COTIZACIONES: MODULO_COTIZACIONES,
  CREDITOS: MODULO_CREDITO,
  GUIAS: MODULO_GUIAS,
  RECETAS: MODULO_RECETAS,
  VEHICULOS: MODULO_VEHICULOS,
  ORDENES: MODULO_ORDENES_SERVICIO,
  PAQUETES: MODULO_PAQUETES,
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
  // Lubricentro: orden de servicio que se va a cobrar en el punto de venta.
  const [ordenACargar, setOrdenACargar] = useState(null);
  // Cambio de prenda en curso: lo que el cliente devuelve; lo que se lleva
  // se elige en el punto de venta.
  const [cambioEnCurso, setCambioEnCurso] = useState(null);
  const [abrirPedido, setAbrirPedido] = useState(null);
  const [pedidosListos, setPedidosListos] = useState(0);

  // Dirección propia del negocio (monspeet.pages.dev/verane/caja), leída al
  // abrir la página. Ver utils/direccion.js.
  const [direccionInicial] = useState(leerDireccion);
  // Pantalla escrita en la dirección: se abre al entrar (una sola vez).
  const pantallaPedida = useRef(direccionInicial.pantalla);
  // Mientras se averigua de qué negocio es la dirección escrita. No hace
  // falta si es la del negocio que este navegador ya recuerda.
  const [resolviendoDireccion, setResolviendoDireccion] = useState(() => {
    const { slug } = direccionInicial;
    if (!slug) return false;
    try {
      const recordada = JSON.parse(localStorage.getItem(TIENDA_STORAGE_KEY) || 'null');
      return !(recordada && (recordada.slug === slug || recordada.identificador === slug));
    } catch {
      return true;
    }
  });
  // La dirección es de otro negocio que el de la sesión abierta.
  const [otroNegocio, setOtroNegocio] = useState(null);

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
          if (pantallaPedida.current) {
            setPantalla(pantallaPedida.current);
            pantallaPedida.current = null;
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
    // Si se entró por una dirección con pantalla (/verane/caja), se abre esa.
    if (pantallaPedida.current) {
      setPantalla(pantallaPedida.current);
      pantallaPedida.current = null;
    }
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

  // La dirección escrita (/verane): sin sesión, el login sale con el logo y
  // el color de ese negocio; con la sesión de otro negocio, se pregunta.
  useEffect(() => {
    if (verificandoSesion || !resolviendoDireccion) return;
    const { slug } = direccionInicial;
    let recordada = null;
    try {
      recordada = JSON.parse(localStorage.getItem(TIENDA_STORAGE_KEY) || 'null');
    } catch {
      recordada = null;
    }
    let vigente = true;
    api
      .negocioPublico(slug)
      .then((negocioDeLaDireccion) => {
        if (!vigente || !negocioDeLaDireccion) return;
        if (negocioDeLaDireccion.identificador === recordada?.identificador) return;
        if (logueado) setOtroNegocio(negocioDeLaDireccion);
        else handleTiendaIdentificada(negocioDeLaDireccion);
      })
      .catch(() => {})
      .finally(() => {
        if (vigente) setResolviendoDireccion(false);
      });
    return () => {
      vigente = false;
    };
    // Solo al abrir la página, cuando ya se sabe si hay sesión.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [verificandoSesion]);

  // Con sesión: se trae la dirección del negocio (las sesiones de antes no
  // la tienen, o pudo cambiarse desde el panel).
  const identificadorEnSesion = logueado ? tiendaRecordada?.identificador : null;
  useEffect(() => {
    if (!identificadorEnSesion) return;
    api
      .negocioPublico(identificadorEnSesion)
      .then((negocioPropio) => {
        if (!negocioPropio || negocioPropio.identificador !== identificadorEnSesion) return;
        setTiendaRecordada((actual) => {
          if (!actual || actual.identificador !== identificadorEnSesion || actual.slug === negocioPropio.slug) return actual;
          const actualizada = { ...actual, slug: negocioPropio.slug };
          localStorage.setItem(TIENDA_STORAGE_KEY, JSON.stringify(actualizada));
          return actualizada;
        });
      })
      .catch(() => {});
  }, [identificadorEnSesion]);

  // Sin sesión: la dirección es la del negocio recordado (/verane) o la raíz.
  const slugRecordado = tiendaRecordada?.slug || null;
  useEffect(() => {
    if (verificandoSesion || logueado || resolviendoDireccion || direccionInicial.delSistema) return;
    irADireccion(slugRecordado ? `/${slugRecordado}` : '/');
  }, [verificandoSesion, logueado, resolviendoDireccion, slugRecordado, direccionInicial.delSistema]);

  // App instalable con el nombre y el logo de este negocio (utils/appInstalable.js).
  const logoDelNegocio = identidadNegocio.logo;
  useEffect(() => {
    if (slugRecordado) usarAppDelNegocio(slugRecordado, { conLogo: !!logoDelNegocio });
  }, [slugRecordado, logoDelNegocio]);

  if (/^\/panel\/?$/.test(window.location.pathname)) {
    return (
      <Suspense fallback={null}>
        <Panel />
      </Suspense>
    );
  }

  const matchBoletaPublica = window.location.pathname.match(/^\/boleta\/([^/]+)\/(\d+)$/);
  if (matchBoletaPublica) {
    const [, identificadorUrl, comprobanteIdUrl] = matchBoletaPublica;
    return <BoletaPublica identificador={identificadorUrl} comprobanteId={comprobanteIdUrl} />;
  }

  // Mientras se averigua de quién es la dirección escrita (un instante, y
  // solo si no es la del negocio recordado) no se muestra nada.
  if (verificandoSesion || resolviendoDireccion) {
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

  // Se abrió la dirección de otro negocio con una sesión ya abierta: nunca
  // se mezclan; se elige con cuál seguir.
  if (otroNegocio) {
    return (
      <div className="login-tienda-container">
        <div className="login-tienda-form">
          <h1 className="login-tienda-nombre">{otroNegocio.nombre_negocio}</h1>
          <p className="login-tienda-subtitulo">
            Esta dirección es de {otroNegocio.nombre_negocio}, pero en este navegador está abierta la sesión de {nombreTienda}.
          </p>
          <button
            type="button"
            className="login-tienda-submit"
            onClick={() => {
              const destino = otroNegocio;
              setOtroNegocio(null);
              handleLogout();
              handleTiendaIdentificada(destino);
            }}
          >
            Cerrar sesión y entrar a {otroNegocio.nombre_negocio}
          </button>
          <button type="button" className="login-tienda-cambiar" onClick={() => setOtroNegocio(null)}>
            Seguir en {nombreTienda}
          </button>
        </div>
      </div>
    );
  }

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
      <DireccionNegocio slug={tiendaRecordada?.slug} pantalla={pantallaVisible} onCambiarPantalla={setPantalla} />
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
              presentaciones={negocio.modulos.includes(MODULO_PRESENTACIONES)}
              precioMayor={negocio.modulos.includes(MODULO_PRECIO_MAYOR)}
              fichaMedicamento={negocio.modulos.includes(MODULO_FICHA_MEDICAMENTO)}
              recetas={negocio.modulos.includes(MODULO_RECETAS)}
              alternativas={negocio.modulos.includes(MODULO_ALTERNATIVAS)}
              fichaRepuesto={negocio.modulos.includes(MODULO_FICHA_REPUESTO)}
              compatibilidad={negocio.modulos.includes(MODULO_COMPATIBILIDAD)}
              vehiculos={negocio.modulos.includes(MODULO_VEHICULOS)}
              paquetes={negocio.modulos.includes(MODULO_PAQUETES)}
              ordenACargar={negocio.modulos.includes(MODULO_ORDENES_SERVICIO) ? ordenACargar : null}
              onOrdenUsada={() => setOrdenACargar(null)}
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
            presentaciones={negocio.modulos.includes(MODULO_PRESENTACIONES)}
            precioMayor={negocio.modulos.includes(MODULO_PRECIO_MAYOR)}
            fichaMedicamento={negocio.modulos.includes(MODULO_FICHA_MEDICAMENTO)}
            fichaRepuesto={negocio.modulos.includes(MODULO_FICHA_REPUESTO)}
            compatibilidad={negocio.modulos.includes(MODULO_COMPATIBILIDAD)}
            ganancias={negocio.modulos.includes(MODULO_GANANCIAS)}
            verSinPrecio={productosSinPrecio}
            onSinPrecioVisto={() => setProductosSinPrecio(false)}
          />
        )}
        {pantallaVisible === 'STOCK' && <StockLotes esAdmin={usuarioActual?.rol_id === 1} />}
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
        {pantallaVisible === 'VEHICULOS' && <Vehiculos nombreTienda={nombreTienda} />}
        {pantallaVisible === 'ORDENES' && (
          <Ordenes
            esAdmin={usuarioActual?.rol_id === 1}
            conPaquetes={negocio.modulos.includes(MODULO_PAQUETES)}
            nombreTienda={nombreTienda}
            direccion={configuracionTienda?.direccion}
            telefono={configuracionTienda?.telefono}
            onCobrar={(orden) => {
              setPedidoACobrar(null);
              setCotizacionACargar(null);
              setOrdenACargar(orden);
              setPantalla('POS');
            }}
          />
        )}
        {pantallaVisible === 'PAQUETES' && usuarioActual?.rol_id === 1 && <Paquetes />}
        {pantallaVisible === 'CREDITOS' && (
          <Creditos
            nombreTienda={nombreTienda}
            direccion={configuracionTienda?.direccion}
            telefono={configuracionTienda?.telefono}
            ruc={configuracionTienda?.ruc}
          />
        )}
        {pantallaVisible === 'GUIAS' && (
          <Guias
            esAdmin={usuarioActual?.rol_id === 1}
            nombreTienda={nombreTienda}
            direccion={configuracionTienda?.direccion}
            telefono={configuracionTienda?.telefono}
            ruc={configuracionTienda?.ruc}
          />
        )}
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
        {pantallaVisible === 'GASTOS' && usuarioActual?.rol_id === 1 && <Gastos />}
        {pantallaVisible === 'RECETAS' && usuarioActual?.rol_id === 1 && <LibroRecetas />}
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
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Camera, MessageCircle, Pencil, Plus, Printer, ShoppingCart, Trash2 } from 'lucide-react';
import { api, API_URL } from '../../api/api';
import { fechaCorta, hoyLima } from '../../utils/formato';
import { formatoCantidad } from '../../utils/medidas';
import { abreviaturaUnidad } from '../../utils/unidades';
import { METODOS_DE_ABONO, nombreMetodo } from '../../utils/metodoPago';
import { enlaceWhatsapp } from '../../utils/vehiculos';
import { imprimirTicket } from '../../utils/imprimirTicket';
import { confirmar } from '../../utils/confirmar';
import { comprimirImagen } from '../../utils/comprimirImagen';
import { esRespaldoLavanderia, textosEncargos } from '../../utils/textosEncargos';
import '../../components/PantallaModulo.css';
import '../../components/Recibo.css';
import './Encargos.css';

const FILTROS = [
  { valor: 'ACTIVOS', label: 'Por entregar' },
  { valor: 'ENTREGADOS', label: 'Entregados' },
  { valor: 'CANCELADOS', label: 'Cancelados' },
  { valor: 'TODOS', label: 'Todos' },
];

const ETIQUETA_ESTADO = {
  PENDIENTE: ['Pendiente', 'pm-chip-aviso'],
  EN_PREPARACION: ['En preparación', 'pm-chip-aviso'],
  LISTO: ['Listo', 'pm-chip-ok'],
  ENTREGADO: ['Entregado', ''],
  CANCELADO: ['Cancelado', 'pm-chip-mal'],
};

const soles = (n) => `S/ ${Number(n || 0).toFixed(2)}`;

const DIAS = ['domingo', 'lunes', 'martes', 'miércoles', 'jueves', 'viernes', 'sábado'];
// "martes 27/10/2026"
const diaYFecha = (texto) => {
  const [a, m, d] = String(texto || '').slice(0, 10).split('-').map(Number);
  if (!a || !m || !d) return fechaCorta(texto);
  return `${DIAS[new Date(Date.UTC(a, m - 1, d)).getUTCDay()]} ${fechaCorta(texto)}`;
};

/**
 * Lavandería: el WhatsApp lleva la orden completa. Recibida o en lavado:
 * las prendas, el total, lo pagado, el saldo y cuándo se entrega. Lista:
 * que ya puede recogerla (o que sale el delivery) y lo que falta pagar.
 */
function mensajeLavanderia(encargo, tienda) {
  const nombre = encargo.cliente_nombre.split(' ')[0];
  const pagado = redondear2(encargo.adelantado - encargo.devuelto);
  const cuenta = [
    `Total: ${soles(encargo.total)}`,
    ...(pagado > 0 ? [`Pagado: ${soles(pagado)}`] : []),
    encargo.saldo > 0 ? `Saldo por pagar: ${soles(encargo.saldo)}` : 'Ya está todo pagado ✅',
  ];
  if (encargo.estado === 'LISTO') {
    return [
      `Hola ${nombre} 👋`,
      `¡Tu ropa ya está lista! 🧺 Orden N° ${encargo.numero} de ${tienda}.`,
      '',
      encargo.entrega === 'DELIVERY'
        ? `La llevamos a ${encargo.direccion_entrega || 'tu dirección'}.`
        : 'Ya puedes pasar a recogerla.',
      encargo.saldo > 0 ? `Saldo por pagar: ${soles(encargo.saldo)}` : 'Ya está todo pagado ✅',
      '',
      '¡Gracias por tu preferencia!',
    ].join('\n');
  }
  const prendas = (encargo.items || []).map((i) => {
    const unidad = i.unidad_medida === 'PRENDA' ? (Number(i.cantidad) === 1 ? 'prenda' : 'prendas') : abreviaturaUnidad(i.unidad_medida);
    const cantidad = `${formatoCantidad(i.cantidad)} ${unidad}`.trim();
    return `• ${i.nombre}${i.detalle ? ` (${i.detalle})` : ''} · ${cantidad} · ${soles(i.total)}`;
  });
  const entrega =
    encargo.entrega === 'DELIVERY'
      ? `Entrega a domicilio: ${diaYFecha(encargo.fecha_entrega)}${encargo.hora_entrega ? ` a las ${encargo.hora_entrega}` : ''}${
          encargo.direccion_entrega ? `, en ${encargo.direccion_entrega}` : ''
        }.`
      : `Lista para recoger: ${diaYFecha(encargo.fecha_entrega)}${encargo.hora_entrega ? ` desde las ${encargo.hora_entrega}` : ''}.`;
  return [
    `Hola ${nombre} 👋`,
    `Recibimos tu ropa en ${tienda}. Orden N° ${encargo.numero}:`,
    '',
    ...prendas,
    '',
    ...cuenta,
    '',
    entrega,
    'Te avisamos por aquí cuando esté lista. ¡Gracias!',
  ].join('\n');
}
const numeroDe = (texto) => Number(String(texto ?? '').replace(',', '.'));
const redondear2 = (n) => Math.round(n * 100) / 100;

// t: textos del rubro (utils/textosEncargos.js); "En lavado" en lavandería.
function Estado({ estado, t }) {
  const [base, clase] = ETIQUETA_ESTADO[estado] || [estado, ''];
  const texto = t?.estados?.[estado] || base;
  return <span className={`pm-chip ${clase}`}>{texto}</span>;
}

/** "Hoy", "Mañana", "Atrasado" o la fecha. */
function cuando(fecha) {
  const hoy = hoyLima();
  if (fecha < hoy) return 'Atrasado';
  if (fecha === hoy) return 'Hoy';
  if (fecha === hoyLima(1)) return 'Mañana';
  const dia = new Date(`${fecha}T12:00:00`).toLocaleDateString('es-PE', { weekday: 'long', day: 'numeric', month: 'long' });
  return dia.charAt(0).toUpperCase() + dia.slice(1);
}

/** Pedidos por entregar agrupados por día: atrasados, hoy, mañana y los demás. */
function agrupar(lista) {
  const grupos = [];
  for (const e of lista) {
    const titulo = cuando(e.fecha_entrega);
    const ultimo = grupos[grupos.length - 1];
    if (ultimo && ultimo.titulo === titulo) ultimo.items.push(e);
    else grupos.push({ titulo, items: [e] });
  }
  return grupos;
}

const resumenItems = (e) =>
  (e.items || []).map((i) => `${formatoCantidad(i.cantidad)} ${i.nombre}`).join(', ');

/**
 * Pedidos por encargo (módulo ENCARGOS, pastelería; en lavandería son las
 * órdenes de lavado, con sus propios textos). El cliente pide una
 * torta para una fecha y deja un adelanto: el adelanto no es una venta ni
 * lleva comprobante; queda en el historial del pedido (y en la caja, si fue
 * en efectivo). Al entregarlo se cobra en el punto de venta: sale la boleta
 * por el total y solo se cobra el saldo.
 */
export default function Encargos({ nombreTienda, direccion, telefono, onEntregar, etiquetas }) {
  const t = useMemo(() => textosEncargos(etiquetas), [etiquetas]);
  const IconoTitulo = t.Icono;
  const [filtro, setFiltro] = useState('ACTIVOS');
  const [buscar, setBuscar] = useState('');
  const [lista, setLista] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [abierto, setAbierto] = useState(null);
  const [formulario, setFormulario] = useState(null);
  const [impresion, setImpresion] = useState(null);

  const cargar = useCallback(() => {
    setCargando(true);
    api
      .encargos(filtro, buscar.trim())
      .then((d) => {
        setLista(d);
        setError(null);
      })
      .catch((e) => setError(e.message))
      .finally(() => setCargando(false));
  }, [filtro, buscar]);

  useEffect(() => {
    const t = setTimeout(cargar, buscar ? 300 : 0);
    return () => clearTimeout(t);
  }, [cargar, buscar]);

  // Imprime el ticket apenas se dibuja (comprobante del pedido u orden de producción).
  useEffect(() => {
    if (!impresion) return undefined;
    const t = setTimeout(() => imprimirTicket(), 80);
    return () => clearTimeout(t);
  }, [impresion]);

  const abrir = async (id) => {
    setError(null);
    try {
      setAbierto(await api.encargo(id));
    } catch (e) {
      setError(e.message);
    }
  };

  const alGuardar = (encargo) => {
    setFormulario(null);
    setAbierto(encargo);
    cargar();
  };

  const grupos = filtro === 'ACTIVOS' ? agrupar(lista) : [{ titulo: null, items: lista }];

  return (
    <div className="pm-layout">
      <header className="pm-cabecera">
        <div>
          <h1>
            <IconoTitulo size={22} /> {t.titulo}
          </h1>
          <p className="pm-subtitulo">{t.subtitulo}</p>
        </div>
        <div className="pm-acciones">
          <button className="pm-boton" onClick={() => setFormulario({})}>
            <Plus size={15} /> {t.nuevo}
          </button>
        </div>
      </header>

      {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}

      <div className="enc-barra">
        <div className="pm-filtros" role="tablist">
          {FILTROS.map((f) => (
            <button key={f.valor} role="tab" aria-selected={filtro === f.valor} className={filtro === f.valor ? 'activo' : ''} onClick={() => setFiltro(f.valor)}>
              {f.label}
            </button>
          ))}
        </div>
        <input
          className="enc-buscar"
          value={buscar}
          placeholder="Buscar por cliente, teléfono o N°"
          onChange={(e) => setBuscar(e.target.value)}
          aria-label={t.buscarListado}
        />
      </div>

      {cargando && lista.length === 0 ? (
        <div className="pm-tarjeta">
          <p className="pm-vacio">Cargando...</p>
        </div>
      ) : lista.length === 0 ? (
        <div className="pm-tarjeta">
          <p className="pm-vacio">
            {buscar
              ? t.sinCoincidencias
              : filtro === 'ACTIVOS'
                ? t.sinPendientes
                : t.sinNada}
          </p>
        </div>
      ) : (
        grupos.map((g) => (
          <section key={g.titulo || 'todos'} className="enc-grupo">
            {g.titulo && (
              <h2 className={`enc-grupo-titulo${g.titulo === 'Atrasado' ? ' enc-atrasado' : ''}`}>
                {g.titulo}
                <span>{g.items.length}</span>
              </h2>
            )}
            <div className="pm-tarjeta">
              {g.items.map((e) => (
                <button key={e.id} className="pm-fila" onClick={() => abrir(e.id)}>
                  <span className="pm-fila-principal">
                    <span className="pm-fila-titulo">
                      {e.hora_entrega && <span className="enc-hora">{e.hora_entrega}</span>}N° {e.numero} · {e.cliente_nombre} <Estado estado={e.estado} t={t} />
                      {e.entrega === 'DELIVERY' && <span className="pm-chip">Delivery</span>}
                    </span>
                    <span className="pm-fila-detalle" style={{ display: 'block' }}>
                      {filtro !== 'ACTIVOS' && `${fechaCorta(e.fecha_entrega)} · `}
                      {resumenItems(e)}
                    </span>
                  </span>
                  <span className="pm-fila-monto">
                    <strong>{soles(e.total)}</strong>
                    {e.estado !== 'ENTREGADO' && e.estado !== 'CANCELADO' && (
                      <small className={e.saldo > 0 ? 'enc-saldo' : 'enc-pagado'}>{e.saldo > 0 ? `Saldo ${soles(e.saldo)}` : 'Pagado'}</small>
                    )}
                  </span>
                </button>
              ))}
            </div>
          </section>
        ))
      )}

      {formulario && (
        <FormularioEncargo t={t} encargo={formulario.encargo || null} onCerrar={() => setFormulario(null)} onGuardado={alGuardar} />
      )}

      {abierto && !formulario && (
        <DetalleEncargo
          t={t}
          encargo={abierto}
          nombreTienda={nombreTienda}
          onCambio={(e) => {
            setAbierto(e);
            cargar();
          }}
          onCerrar={() => setAbierto(null)}
          onEditar={() => setFormulario({ encargo: abierto })}
          onImprimir={(tipo) => setImpresion({ tipo, encargo: abierto, n: Date.now() })}
          onEntregar={onEntregar}
        />
      )}

      {impresion &&
        createPortal(
          <EncargoImprimible
            key={impresion.n}
            t={t}
            tipo={impresion.tipo}
            encargo={impresion.encargo}
            nombreTienda={nombreTienda}
            direccion={direccion}
            telefono={telefono}
          />,
          document.body
        )}
    </div>
  );
}

// ---------------------------------------------------------------------
// Nuevo pedido / editar
// ---------------------------------------------------------------------

const lineaDeItem = (i) => ({
  producto_id: i.producto_id,
  nombre: i.nombre,
  // Lavandería: prenda escrita en la orden (su servicio es un respaldo).
  libre: !!i.libre,
  cobro: i.unidad_medida === 'KG' ? 'KG' : 'PRENDA',
  unidad_medida: i.unidad_medida,
  cantidad: String(formatoCantidad(i.cantidad)),
  precio: String(i.precio_unitario),
  detalle: i.detalle || '',
});

function FormularioEncargo({ t, encargo, onCerrar, onGuardado }) {
  const editando = !!encargo;
  const [datos, setDatos] = useState(() => ({
    cliente: encargo?.cliente_id
      ? { id: encargo.cliente_id, nombre_razon_social: encargo.cliente_nombre, numero_documento: encargo.cliente_documento }
      : null,
    cliente_nombre: encargo && !encargo.cliente_id ? encargo.cliente_nombre : '',
    cliente_telefono: encargo?.cliente_telefono || '',
    fecha_entrega: encargo?.fecha_entrega || hoyLima(1),
    hora_entrega: encargo?.hora_entrega || '',
    entrega: encargo?.entrega || 'RECOJO',
    direccion_entrega: encargo?.direccion_entrega || '',
    dedicatoria: encargo?.dedicatoria || '',
    notas: encargo?.notas || '',
    items: (encargo?.items || []).map(lineaDeItem),
    adelanto: '',
    adelanto_metodo: 'EFECTIVO',
  }));
  const [productos, setProductos] = useState([]);
  const [buscarProducto, setBuscarProducto] = useState('');
  const [buscarCliente, setBuscarCliente] = useState('');
  const [clientes, setClientes] = useState([]);
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const cambiar = (campo, valor) => setDatos((d) => ({ ...d, [campo]: valor }));

  useEffect(() => {
    api
      .productos()
      .then((lista) => setProductos((lista || []).filter((p) => !esRespaldoLavanderia(p))))
      .catch(() => {});
  }, []);

  useEffect(() => {
    const q = buscarCliente.trim();
    if (q.length < 2) return undefined;
    const t = setTimeout(() => {
      api
        .clientesBuscar(q)
        .then((r) => setClientes((r || []).slice(0, 6)))
        .catch(() => setClientes([]));
    }, 250);
    return () => clearTimeout(t);
  }, [buscarCliente]);

  const encontrados = useMemo(() => {
    const q = buscarProducto.trim().toLowerCase();
    if (q.length < 2) return [];
    return productos.filter((p) => p.nombre.toLowerCase().includes(q) || String(p.codigo || '').toLowerCase().includes(q)).slice(0, 8);
  }, [productos, buscarProducto]);

  // Lavandería: los servicios del negocio como botones (Lavado por kilo,
  // Al seco, Planchado...), para agregarlos de un toque al recibir la ropa.
  const rapidos = useMemo(() => (t.serviciosRapidos ? productos.slice(0, 12) : []), [t.serviciosRapidos, productos]);

  const agregar = (p) => {
    setBuscarProducto('');
    setDatos((d) => ({
      ...d,
      items: [
        ...d.items,
        { producto_id: p.id, nombre: p.nombre, unidad_medida: p.unidad_medida, cantidad: '1', precio: String(p.precio), detalle: '' },
      ],
    }));
  };
  // Lavandería: una prenda que no está en Servicios se escribe aquí mismo,
  // con su cobro (por prenda o por kilo) y su precio.
  const agregarEscrita = () =>
    setDatos((d) => ({
      ...d,
      items: [
        ...d.items,
        { producto_id: null, libre: true, cobro: 'PRENDA', nombre: '', unidad_medida: 'PRENDA', cantidad: '1', precio: '', detalle: '' },
      ],
    }));
  // Cambiar el cobro de una prenda escrita: el servidor le asigna el respaldo que corresponde.
  const cambiarCobro = (idx, cobro) =>
    setDatos((d) => ({
      ...d,
      items: d.items.map((i, n) =>
        n === idx && i.cobro !== cobro ? { ...i, cobro, unidad_medida: cobro === 'KG' ? 'KG' : 'PRENDA', producto_id: null } : i
      ),
    }));
  const cambiarItem = (idx, campo, valor) =>
    setDatos((d) => ({ ...d, items: d.items.map((i, n) => (n === idx ? { ...i, [campo]: valor } : i)) }));
  const quitarItem = (idx) => setDatos((d) => ({ ...d, items: d.items.filter((_, n) => n !== idx) }));

  const total = redondear2(datos.items.reduce((s, i) => s + redondear2((numeroDe(i.precio) || 0) * (numeroDe(i.cantidad) || 0)), 0));
  const adelanto = numeroDe(datos.adelanto) || 0;

  const guardar = async () => {
    setError(null);
    if (!datos.cliente && !datos.cliente_nombre.trim()) return setError('Elige un cliente o escribe su nombre.');
    if (datos.items.length === 0) return setError(t.errorSinItems);
    if (datos.items.some((i) => i.libre && !i.nombre.trim())) return setError(t.errorPrendaSinNombre);
    if (datos.items.some((i) => !(numeroDe(i.cantidad) > 0) || !(numeroDe(i.precio) >= 0))) return setError('Revisa las cantidades y los precios.');
    if (datos.entrega === 'DELIVERY' && !datos.direccion_entrega.trim()) return setError('Escribe la dirección del delivery.');
    if (!editando && adelanto > total + 0.005) return setError('El adelanto no puede ser mayor que el total.');
    const cuerpo = {
      cliente_id: datos.cliente?.id || null,
      cliente_nombre: datos.cliente ? null : datos.cliente_nombre.trim(),
      cliente_telefono: datos.cliente_telefono.trim() || null,
      fecha_entrega: datos.fecha_entrega,
      hora_entrega: datos.hora_entrega || null,
      entrega: datos.entrega,
      direccion_entrega: datos.entrega === 'DELIVERY' ? datos.direccion_entrega.trim() : null,
      dedicatoria: datos.dedicatoria.trim() || null,
      notas: datos.notas.trim() || null,
      items: datos.items.map((i) => ({
        producto_id: i.producto_id,
        ...(i.libre ? { nombre: i.nombre.trim(), cobro: i.cobro } : {}),
        cantidad: numeroDe(i.cantidad),
        precio_unitario: numeroDe(i.precio),
        detalle: i.detalle.trim() || null,
      })),
      ...(!editando && adelanto > 0 ? { adelanto: { monto: adelanto, metodo_pago: datos.adelanto_metodo } } : {}),
    };
    setOcupado(true);
    try {
      onGuardado(editando ? await api.encargoActualizar(encargo.id, cuerpo) : await api.encargoCrear(cuerpo));
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && !ocupado && onCerrar()}>
      <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label={editando ? t.ariaEditar : t.ariaNuevo}>
        <div className="pm-modal-cabecera">
          <h2>{editando ? t.tituloEditar(encargo.numero) : t.tituloNuevo}</h2>
          <p>{t.ayudaPrecio}</p>
        </div>
        <div className="pm-modal-cuerpo">
          {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}

          <p className="pm-seccion">Cliente</p>
          {datos.cliente ? (
            <div className="enc-cliente-elegido">
              <span>
                <strong>{datos.cliente.nombre_razon_social}</strong>
                {datos.cliente.numero_documento && <small> · {datos.cliente.numero_documento}</small>}
              </span>
              <button type="button" className="pm-boton-secundario" onClick={() => cambiar('cliente', null)}>
                Cambiar
              </button>
            </div>
          ) : (
            <div className="pm-campos">
              <label className="pm-campo pm-campo-ancho">
                <span>Buscar cliente registrado (opcional)</span>
                <input value={buscarCliente} placeholder="Nombre o DNI" onChange={(e) => setBuscarCliente(e.target.value)} />
              </label>
              {buscarCliente.trim().length >= 2 && clientes.length > 0 && (
                <div className="pm-campo-ancho">
                  {clientes.map((c) => (
                    <button
                      key={c.id}
                      type="button"
                      className="pm-fila"
                      onClick={() => {
                        setDatos((d) => ({ ...d, cliente: c, cliente_telefono: d.cliente_telefono || c.telefono || '' }));
                        setBuscarCliente('');
                        setClientes([]);
                      }}
                    >
                      <span className="pm-fila-principal">
                        <span className="pm-fila-titulo">{c.nombre_razon_social}</span>
                        <span className="pm-fila-detalle" style={{ display: 'block' }}>
                          {[c.numero_documento, c.telefono].filter(Boolean).join(' · ')}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              )}
              <label className="pm-campo">
                <span>Nombre</span>
                <input value={datos.cliente_nombre} maxLength={150} onChange={(e) => cambiar('cliente_nombre', e.target.value)} />
              </label>
            </div>
          )}
          <div className="pm-campos">
            <label className="pm-campo">
              <span>Celular (para avisarle)</span>
              <input
                inputMode="tel"
                value={datos.cliente_telefono}
                maxLength={20}
                onChange={(e) => cambiar('cliente_telefono', e.target.value.replace(/[^\d+ ]/g, ''))}
              />
            </label>
          </div>

          <p className="pm-seccion">Entrega</p>
          <div className="pm-campos">
            <label className="pm-campo">
              <span>Fecha</span>
              <input type="date" min={editando ? undefined : hoyLima()} value={datos.fecha_entrega} onChange={(e) => cambiar('fecha_entrega', e.target.value)} />
            </label>
            <label className="pm-campo">
              <span>Hora (opcional)</span>
              <input type="time" value={datos.hora_entrega} onChange={(e) => cambiar('hora_entrega', e.target.value)} />
            </label>
            <div className="pm-campo pm-campo-ancho">
              <div className="pm-opciones">
                {[
                  ['RECOJO', 'Recoge en tienda'],
                  ['DELIVERY', 'Delivery'],
                ].map(([valor, label]) => (
                  <button key={valor} type="button" className={datos.entrega === valor ? 'activo' : ''} onClick={() => cambiar('entrega', valor)}>
                    {label}
                  </button>
                ))}
              </div>
            </div>
            {datos.entrega === 'DELIVERY' && (
              <label className="pm-campo pm-campo-ancho">
                <span>Dirección de entrega</span>
                <input value={datos.direccion_entrega} maxLength={250} onChange={(e) => cambiar('direccion_entrega', e.target.value)} />
              </label>
            )}
          </div>

          <p className="pm-seccion">{t.seccionItems}</p>
          {t.serviciosRapidos && rapidos.length > 0 && (
            <div className="pm-opciones enc-rapidos" aria-label="Agregar un servicio">
              {rapidos.map((p) => (
                <button key={p.id} type="button" onClick={() => agregar(p)}>
                  <Plus size={13} /> {p.nombre}
                </button>
              ))}
            </div>
          )}
          {t.prendasEscritas && (
            <button type="button" className="pm-boton-secundario enc-prenda-nueva" onClick={agregarEscrita}>
              <Plus size={14} /> {t.botonPrendaNueva}
            </button>
          )}
          {datos.items.length === 0 && <p className="pm-vacio">{t.vacioItems}</p>}
          {datos.items.map((i, idx) => (
            <div key={`${i.producto_id}-${idx}`} className="enc-item">
              <div className="enc-item-cabecera">
                {i.libre ? (
                  <label className="pm-campo enc-item-prenda">
                    <span>{t.etiquetaPrenda}</span>
                    <input
                      value={i.nombre}
                      maxLength={120}
                      placeholder={t.ejemploPrenda}
                      autoFocus={!i.nombre && idx === datos.items.length - 1}
                      onChange={(e) => cambiarItem(idx, 'nombre', e.target.value)}
                    />
                  </label>
                ) : (
                  <strong>{i.nombre}</strong>
                )}
                <button type="button" className="pm-boton-secundario" onClick={() => quitarItem(idx)} aria-label={`Quitar ${i.nombre}`}>
                  <Trash2 size={14} />
                </button>
              </div>
              {i.libre && (
                <div className="pm-opciones enc-item-cobro" aria-label="Cómo se cobra">
                  {[
                    ['PRENDA', 'Por prenda'],
                    ['KG', 'Por kilo'],
                  ].map(([valor, label]) => (
                    <button key={valor} type="button" className={i.cobro === valor ? 'activo' : ''} onClick={() => cambiarCobro(idx, valor)}>
                      {label}
                    </button>
                  ))}
                </div>
              )}
              <div className="enc-item-campos">
                <label className="pm-campo">
                  <span>Cantidad {abreviaturaUnidad(i.unidad_medida)}</span>
                  <input inputMode="decimal" value={i.cantidad} onChange={(e) => cambiarItem(idx, 'cantidad', e.target.value)} />
                </label>
                <label className="pm-campo">
                  <span>{t.etiquetaPrecio}</span>
                  <input inputMode="decimal" value={i.precio} onChange={(e) => cambiarItem(idx, 'precio', e.target.value)} />
                </label>
                <label className="pm-campo enc-item-detalle">
                  <span>{i.libre ? t.etiquetaDetalleEscrita : t.etiquetaDetalle}</span>
                  <input
                    value={i.detalle}
                    maxLength={500}
                    placeholder={i.libre ? t.ejemploDetalleEscrita : idx === 0 ? t.ejemploDetalle : 'Opcional'}
                    onChange={(e) => cambiarItem(idx, 'detalle', e.target.value)}
                  />
                </label>
              </div>
            </div>
          ))}
          <div className="pm-campo" style={{ marginTop: 8 }}>
            <input value={buscarProducto} placeholder={t.buscarProducto} onChange={(e) => setBuscarProducto(e.target.value)} />
          </div>
          {encontrados.map((p) => (
            <button key={p.id} type="button" className="pm-fila" onClick={() => agregar(p)}>
              <span className="pm-fila-principal">
                <span className="pm-fila-titulo">{p.nombre}</span>
                <span className="pm-fila-detalle" style={{ display: 'block' }}>
                  {p.codigo}
                </span>
              </span>
              <span className="pm-fila-monto">
                <strong>{soles(p.precio)}</strong>
              </span>
            </button>
          ))}
          <div className="pm-total">
            <span>{t.totalPedido}</span>
            <strong>{soles(total)}</strong>
          </div>

          <p className="pm-seccion">{t.seccionNotas}</p>
          <div className="pm-campos">
            {(t.dedicatoria || datos.dedicatoria) && (
              <label className="pm-campo pm-campo-ancho">
                <span>Dedicatoria (lo que va escrito)</span>
                <input value={datos.dedicatoria} maxLength={200} placeholder="¡Feliz cumpleaños, Sofía!" onChange={(e) => cambiar('dedicatoria', e.target.value)} />
              </label>
            )}
            <label className="pm-campo pm-campo-ancho">
              <span>Notas</span>
              <textarea rows={2} maxLength={1000} value={datos.notas} onChange={(e) => cambiar('notas', e.target.value)} />
            </label>
          </div>

          {!editando && (
            <>
              <p className="pm-seccion">Adelanto (sin comprobante)</p>
              <div className="pm-campos">
                <label className="pm-campo">
                  <span>Monto</span>
                  <input inputMode="decimal" value={datos.adelanto} placeholder="0.00" onChange={(e) => cambiar('adelanto', e.target.value)} />
                </label>
                <div className="pm-campo pm-campo-ancho">
                  <div className="pm-opciones">
                    {METODOS_DE_ABONO.map((m) => (
                      <button key={m} type="button" className={datos.adelanto_metodo === m ? 'activo' : ''} onClick={() => cambiar('adelanto_metodo', m)}>
                        {nombreMetodo(m)}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
              {adelanto > 0 && total > 0 && (
                <p className="pm-subtitulo">
                  Saldo al entregar: <strong>{soles(Math.max(0, total - adelanto))}</strong>
                  {datos.adelanto_metodo === 'EFECTIVO' && ' · el adelanto en efectivo entra a la caja abierta.'}
                </p>
              )}
            </>
          )}
        </div>
        <div className="pm-modal-pie">
          <button className="pm-boton-secundario" onClick={onCerrar} disabled={ocupado}>
            Cancelar
          </button>
          <button className="pm-boton" onClick={guardar} disabled={ocupado}>
            {ocupado ? 'Guardando...' : editando ? 'Guardar cambios' : t.botonRegistrar}
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Detalle
// ---------------------------------------------------------------------

function DetalleEncargo({ t, encargo, nombreTienda, onCambio, onCerrar, onEditar, onImprimir, onEntregar }) {
  const IconoOrden = t.IconoOrden;
  const activo = !['ENTREGADO', 'CANCELADO'].includes(encargo.estado);
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const [adelanto, setAdelanto] = useState(null);
  const [cancelando, setCancelando] = useState(null);
  const archivoRef = useRef(null);

  const ejecutar = async (accion) => {
    setOcupado(true);
    setError(null);
    try {
      onCambio(await accion());
      return true;
    } catch (e) {
      setError(e.message);
      return false;
    } finally {
      setOcupado(false);
    }
  };

  const registrarAdelanto = async () => {
    const monto = numeroDe(adelanto.monto);
    if (!(monto > 0)) return setError('Escribe el monto del adelanto.');
    if (await ejecutar(() => api.encargoAdelanto(encargo.id, { monto, metodo_pago: adelanto.metodo }))) setAdelanto(null);
  };

  const cancelar = async () => {
    if (!cancelando.motivo.trim()) return setError('Escribe por qué se cancela.');
    const ok = await ejecutar(() =>
      api.encargoCancelar(encargo.id, {
        motivo: cancelando.motivo.trim(),
        devolver: cancelando.devolver,
        metodo_devolucion: cancelando.metodo,
      })
    );
    if (ok) setCancelando(null);
  };

  const subirFoto = async (archivo) => {
    if (!archivo) return;
    // Se achica en el navegador: sube rápido aunque la foto sea pesada.
    await ejecutar(async () => api.encargoSubirFoto(encargo.id, await comprimirImagen(archivo, { ladoMaximo: 1600 })));
    if (archivoRef.current) archivoRef.current.value = '';
  };

  const pagado = redondear2(encargo.adelantado - encargo.devuelto);
  const mensajeListo = `Hola ${encargo.cliente_nombre.split(' ')[0]}, ${t.tuPedido(encargo.numero)} de ${nombreTienda || t.negocio} ${t.yaListo}${
    encargo.entrega === 'DELIVERY' ? ' y sale en camino' : ' para recoger'
  }.${encargo.saldo > 0 ? ` Saldo por pagar: ${soles(encargo.saldo)}.` : ''} ¡Gracias!`;
  const mensajeRecordatorio = `Hola ${encargo.cliente_nombre.split(' ')[0]}, te recordamos ${t.tuPedido(encargo.numero)} de ${
    nombreTienda || t.negocio
  } para el ${fechaCorta(encargo.fecha_entrega)}${encargo.hora_entrega ? ` a las ${encargo.hora_entrega}` : ''}.${
    encargo.saldo > 0 ? ` Saldo por pagar: ${soles(encargo.saldo)}.` : ''
  }`;
  const whatsapp = enlaceWhatsapp(
    encargo.cliente_telefono,
    t.whatsappDetallado
      ? mensajeLavanderia(encargo, nombreTienda || t.negocio)
      : encargo.estado === 'LISTO'
        ? mensajeListo
        : mensajeRecordatorio
  );

  const entregar = async () => {
    if (
      encargo.estado !== 'LISTO' &&
      !(await confirmar({
        titulo: t.avisoNoListo(encargo.numero),
        mensaje: '¿Lo entregas igual? Se abre el punto de venta para cobrar el saldo.',
        textoConfirmar: 'Entregar igual',
        tipo: 'normal',
        icono: 'aviso',
      }))
    )
      return;
    onEntregar(encargo);
  };

  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label={`${t.Pedido} ${encargo.numero}`}>
        <div className="pm-modal-cabecera">
          <h2>
            {t.Pedido} N° {encargo.numero} · {encargo.cliente_nombre} <Estado estado={encargo.estado} t={t} />
          </h2>
          <p>
            Entrega: {cuando(encargo.fecha_entrega)} {fechaCorta(encargo.fecha_entrega)}
            {encargo.hora_entrega ? ` a las ${encargo.hora_entrega}` : ''} · {encargo.entrega === 'DELIVERY' ? 'Delivery' : 'Recoge en tienda'}
            {encargo.folio_venta ? ` · venta ${encargo.folio_venta}` : ''}
          </p>
        </div>
        <div className="pm-modal-cuerpo">
          {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
          {encargo.estado === 'CANCELADO' && encargo.motivo_cancelacion && (
            <p className="pm-mensaje pm-mensaje-error">Cancelado: {encargo.motivo_cancelacion}</p>
          )}

          <div className="enc-detalle">
            <div>
              <dl className="pm-pares">
                {encargo.cliente_telefono && (
                  <>
                    <dt>Celular</dt>
                    <dd>{encargo.cliente_telefono}</dd>
                  </>
                )}
                {encargo.cliente_documento && (
                  <>
                    <dt>Documento</dt>
                    <dd>{encargo.cliente_documento}</dd>
                  </>
                )}
                {encargo.direccion_entrega && (
                  <>
                    <dt>Dirección</dt>
                    <dd>{encargo.direccion_entrega}</dd>
                  </>
                )}
                {encargo.dedicatoria && (
                  <>
                    <dt>Dedicatoria</dt>
                    <dd>“{encargo.dedicatoria}”</dd>
                  </>
                )}
                {encargo.notas && (
                  <>
                    <dt>Notas</dt>
                    <dd>{encargo.notas}</dd>
                  </>
                )}
                <dt>Registrado</dt>
                <dd>
                  {fechaCorta(encargo.fecha_creacion)}
                  {encargo.usuario ? ` por ${encargo.usuario}` : ''}
                </dd>
              </dl>

              <p className="pm-seccion">{t.seccionItems}</p>
              <table className="pm-lineas">
                <tbody>
                  {encargo.items.map((i) => (
                    <tr key={i.id}>
                      <td>
                        {formatoCantidad(i.cantidad)} {abreviaturaUnidad(i.unidad_medida)} · {i.nombre}
                        {i.detalle && <small>{i.detalle}</small>}
                      </td>
                      <td className="pm-num">{soles(i.total)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="enc-montos">
                <div>
                  <span>Total</span>
                  <strong>{soles(encargo.total)}</strong>
                </div>
                <div>
                  <span>Adelantado</span>
                  <strong>{soles(pagado)}</strong>
                </div>
                {activo && (
                  <div className="enc-montos-saldo">
                    <span>Saldo al entregar</span>
                    <strong>{soles(encargo.saldo)}</strong>
                  </div>
                )}
                {encargo.estado === 'ENTREGADO' && encargo.saldo > 0 && (
                  <div className={encargo.a_cuenta ? 'enc-montos-saldo' : 'enc-montos-pagado'}>
                    <span>{encargo.a_cuenta ? 'Quedó a cuenta' : 'Cobrado al entregar'}</span>
                    <strong>{soles(encargo.saldo)}</strong>
                  </div>
                )}
              </div>
            </div>

            <div className="enc-foto">
              {encargo.foto_url ? (
                <a href={`${API_URL}${encargo.foto_url}`} target="_blank" rel="noreferrer">
                  <img src={`${API_URL}${encargo.foto_url}`} alt={t.altFoto} />
                </a>
              ) : (
                <div className="enc-foto-vacia">
                  <Camera size={26} />
                  <span>{t.sinFoto}</span>
                </div>
              )}
              {activo && (
                <div className="enc-foto-acciones">
                  <input
                    ref={archivoRef}
                    type="file"
                    accept="image/*"
                    hidden
                    onChange={(e) => subirFoto(e.target.files?.[0])}
                  />
                  <button className="pm-boton-secundario" onClick={() => archivoRef.current?.click()} disabled={ocupado}>
                    <Camera size={14} /> {encargo.foto_url ? 'Cambiar foto' : 'Agregar foto'}
                  </button>
                  {encargo.foto_url && (
                    <button className="pm-boton-secundario" onClick={() => ejecutar(() => api.encargoQuitarFoto(encargo.id))} disabled={ocupado}>
                      Quitar
                    </button>
                  )}
                </div>
              )}
            </div>
          </div>

          <p className="pm-seccion">Adelantos</p>
          {(encargo.pagos || []).length === 0 ? (
            <p className="pm-vacio">Todavía no dejó adelanto.</p>
          ) : (
            <table className="pm-lineas">
              <tbody>
                {encargo.pagos.map((p) => (
                  <tr key={p.id}>
                    <td>
                      {p.tipo === 'DEVOLUCION' ? 'Devolución' : 'Adelanto'} · {nombreMetodo(p.metodo_pago)}
                      <small>
                        {fechaCorta(p.fecha)} {p.fecha.slice(11, 16)}
                        {p.usuario ? ` · ${p.usuario}` : ''}
                        {p.nota ? ` · ${p.nota}` : ''}
                      </small>
                    </td>
                    <td className="pm-num">{p.tipo === 'DEVOLUCION' ? `− ${soles(p.monto)}` : soles(p.monto)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {activo && encargo.saldo > 0 && !adelanto && (
            <button className="pm-boton-secundario" style={{ marginTop: 8 }} onClick={() => setAdelanto({ monto: '', metodo: 'EFECTIVO' })}>
              <Plus size={14} /> Registrar adelanto
            </button>
          )}
          {adelanto && (
            <div className="enc-recuadro">
              <div className="pm-campos">
                <label className="pm-campo">
                  <span>Monto (falta {soles(encargo.saldo)})</span>
                  <input autoFocus inputMode="decimal" value={adelanto.monto} onChange={(e) => setAdelanto((a) => ({ ...a, monto: e.target.value }))} />
                </label>
                <div className="pm-campo pm-campo-ancho">
                  <div className="pm-opciones">
                    {METODOS_DE_ABONO.map((m) => (
                      <button key={m} type="button" className={adelanto.metodo === m ? 'activo' : ''} onClick={() => setAdelanto((a) => ({ ...a, metodo: m }))}>
                        {nombreMetodo(m)}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
              <div className="enc-recuadro-pie">
                <button className="pm-boton-secundario" onClick={() => setAdelanto(null)}>
                  Cancelar
                </button>
                <button className="pm-boton" onClick={registrarAdelanto} disabled={ocupado}>
                  Registrar
                </button>
              </div>
            </div>
          )}

          {cancelando && (
            <div className="enc-recuadro enc-recuadro-peligro">
              <label className="pm-campo pm-campo-ancho">
                <span>¿Por qué se cancela?</span>
                <input autoFocus maxLength={250} value={cancelando.motivo} onChange={(e) => setCancelando((c) => ({ ...c, motivo: e.target.value }))} />
              </label>
              {pagado > 0 && (
                <>
                  <div className="pm-opciones" style={{ marginTop: 8 }}>
                    <button type="button" className={cancelando.devolver ? 'activo' : ''} onClick={() => setCancelando((c) => ({ ...c, devolver: true }))}>
                      Devolver {soles(pagado)}
                    </button>
                    <button type="button" className={!cancelando.devolver ? 'activo' : ''} onClick={() => setCancelando((c) => ({ ...c, devolver: false }))}>
                      No devolver (queda retenido)
                    </button>
                  </div>
                  {cancelando.devolver && (
                    <div className="pm-opciones" style={{ marginTop: 8 }}>
                      {METODOS_DE_ABONO.map((m) => (
                        <button key={m} type="button" className={cancelando.metodo === m ? 'activo' : ''} onClick={() => setCancelando((c) => ({ ...c, metodo: m }))}>
                          {nombreMetodo(m)}
                        </button>
                      ))}
                    </div>
                  )}
                  {cancelando.devolver && cancelando.metodo === 'EFECTIVO' && (
                    <p className="pm-subtitulo">El dinero sale de la caja abierta como retiro.</p>
                  )}
                </>
              )}
              <div className="enc-recuadro-pie">
                <button className="pm-boton-secundario" onClick={() => setCancelando(null)}>
                  Volver
                </button>
                <button className="pm-boton-peligro" onClick={cancelar} disabled={ocupado}>
                  {t.cancelar}
                </button>
              </div>
            </div>
          )}
        </div>
        <div className="pm-modal-pie enc-pie">
          {activo && !cancelando && (
            <button className="pm-boton-peligro" onClick={() => setCancelando({ motivo: '', devolver: pagado > 0, metodo: 'EFECTIVO' })} disabled={ocupado}>
              {t.cancelar}
            </button>
          )}
          {activo && (
            <button className="pm-boton-secundario" onClick={onEditar} disabled={ocupado}>
              <Pencil size={14} /> Editar
            </button>
          )}
          <button className="pm-boton-secundario" onClick={() => onImprimir('PRODUCCION')}>
            <IconoOrden size={15} /> {t.ordenTrabajo}
          </button>
          <button className="pm-boton-secundario" onClick={() => onImprimir('CLIENTE')}>
            <Printer size={15} /> Ticket del cliente
          </button>
          {whatsapp && activo && (
            <a className="pm-boton-secundario" href={whatsapp} target="_blank" rel="noreferrer" style={{ textDecoration: 'none' }}>
              <MessageCircle size={15} /> {encargo.estado === 'LISTO' ? 'Avisar que está listo' : t.whatsappDetallado ? 'Enviar la orden por WhatsApp' : 'Recordar por WhatsApp'}
            </a>
          )}
          {encargo.estado === 'PENDIENTE' && (
            <button className="pm-boton-secundario" onClick={() => ejecutar(() => api.encargoEstado(encargo.id, 'EN_PREPARACION'))} disabled={ocupado}>
              {t.botonPreparacion}
            </button>
          )}
          {(encargo.estado === 'PENDIENTE' || encargo.estado === 'EN_PREPARACION') && (
            <button className="pm-boton-secundario" onClick={() => ejecutar(() => api.encargoEstado(encargo.id, 'LISTO'))} disabled={ocupado}>
              Marcar listo
            </button>
          )}
          {encargo.estado === 'LISTO' && (
            <button className="pm-boton-secundario" onClick={() => ejecutar(() => api.encargoEstado(encargo.id, 'EN_PREPARACION'))} disabled={ocupado}>
              {t.volverPreparacion}
            </button>
          )}
          <button className="pm-boton-secundario" onClick={onCerrar}>
            Cerrar
          </button>
          {activo && (
            <button className="pm-boton" onClick={entregar} disabled={ocupado}>
              <ShoppingCart size={15} /> {encargo.saldo > 0 ? `Entregar y cobrar ${soles(encargo.saldo)}` : 'Entregar'}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Tickets: comprobante del pedido (para el cliente) y orden de producción
// ---------------------------------------------------------------------

/**
 * Ticket del pedido. CLIENTE: lo que pidió, para cuándo, lo adelantado y el
 * saldo (no es comprobante de pago). PRODUCCION: para el pastelero, sin
 * precios, con el detalle y la dedicatoria en grande.
 */
function EncargoImprimible({ t, tipo, encargo, nombreTienda, direccion, telefono }) {
  const produccion = tipo === 'PRODUCCION';
  const pagado = redondear2(encargo.adelantado - encargo.devuelto);
  return (
    <div className="recibo-imprimible">
      {!produccion && (
        <>
          <div className="recibo-centro recibo-nombre-tienda">{nombreTienda}</div>
          {direccion && <div className="recibo-centro recibo-dato-tienda">{direccion}</div>}
          {telefono && <div className="recibo-centro recibo-dato-tienda">Tel: {telefono}</div>}
          <div className="recibo-linea"></div>
        </>
      )}
      <div className="recibo-centro recibo-comprobante-tipo">{produccion ? t.ticketTrabajo : t.ticketCliente}</div>
      <div className="recibo-centro recibo-comprobante-numero">N° {encargo.numero}</div>
      <div className="recibo-linea"></div>
      <div className="recibo-fila-meta">
        <span>Entrega</span>
        <span>
          {fechaCorta(encargo.fecha_entrega)}
          {encargo.hora_entrega ? ` ${encargo.hora_entrega}` : ''}
        </span>
      </div>
      <div className="recibo-fila-meta">
        <span>Modo</span>
        <span>{encargo.entrega === 'DELIVERY' ? 'Delivery' : 'Recoge en tienda'}</span>
      </div>
      <div className="recibo-fila-meta">
        <span>Cliente</span>
        <span>{encargo.cliente_nombre}</span>
      </div>
      {encargo.cliente_telefono && (
        <div className="recibo-fila-meta">
          <span>Celular</span>
          <span>{encargo.cliente_telefono}</span>
        </div>
      )}
      {encargo.direccion_entrega && <div className="recibo-cliente-direccion">{encargo.direccion_entrega}</div>}
      <div className="recibo-linea"></div>
      {encargo.items.map((i) => (
        <div key={i.id} className="recibo-item">
          <div className="recibo-item-nombre">
            {produccion ? `${formatoCantidad(i.cantidad)} × ` : ''}
            {i.nombre}
          </div>
          {i.detalle && <div className="enc-ticket-detalle">{i.detalle}</div>}
          {!produccion && (
            <div className="recibo-item-detalle">
              <span>
                {formatoCantidad(i.cantidad)} x S/.{i.precio_unitario.toFixed(2)}
              </span>
              <span>S/.{i.total.toFixed(2)}</span>
            </div>
          )}
        </div>
      ))}
      {encargo.dedicatoria && (
        <>
          <div className="recibo-linea"></div>
          <div className="recibo-seccion-titulo">DEDICATORIA</div>
          <div className="enc-ticket-dedicatoria">“{encargo.dedicatoria}”</div>
        </>
      )}
      {encargo.notas && (
        <>
          <div className="recibo-seccion-titulo">NOTAS</div>
          <div className="enc-ticket-detalle">{encargo.notas}</div>
        </>
      )}
      {!produccion && (
        <>
          <div className="recibo-linea-doble"></div>
          <div className="recibo-fila-meta">
            <span>Total</span>
            <span>S/.{encargo.total.toFixed(2)}</span>
          </div>
          <div className="recibo-fila-meta">
            <span>Adelanto</span>
            <span>S/.{pagado.toFixed(2)}</span>
          </div>
          {/* Entregado: lo que faltaba ya se cobró en la venta (o quedó a cuenta). */}
          {encargo.estado === 'ENTREGADO' && encargo.saldo > 0 && (
            <div className="recibo-fila-meta">
              <span>{encargo.a_cuenta ? 'A cuenta' : 'Pagado al entregar'}</span>
              <span>S/.{encargo.saldo.toFixed(2)}</span>
            </div>
          )}
          {encargo.estado === 'ENTREGADO' ? (
            <div className="recibo-total">
              <span>{encargo.a_cuenta ? 'DEBE (A CUENTA)' : 'PAGADO'}</span>
              <span>{encargo.a_cuenta ? `S/.${encargo.saldo.toFixed(2)}` : `S/.${encargo.total.toFixed(2)}`}</span>
            </div>
          ) : (
            <div className="recibo-total">
              <span>SALDO</span>
              <span>S/.{encargo.saldo.toFixed(2)}</span>
            </div>
          )}
          {encargo.estado === 'ENTREGADO' && encargo.folio_venta && (
            <div className="recibo-fila-meta">
              <span>Venta</span>
              <span>{encargo.folio_venta}</span>
            </div>
          )}
          <div className="recibo-linea"></div>
          <div className="recibo-centro recibo-disclaimer">
            {t.pieTicket}
          </div>
        </>
      )}
      {produccion && encargo.foto_url && <div className="recibo-centro recibo-disclaimer">{t.fotoEnSistema}</div>}
    </div>
  );
}

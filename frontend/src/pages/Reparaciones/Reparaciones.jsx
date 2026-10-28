import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  Camera,
  ClipboardList,
  Eye,
  EyeOff,
  Gamepad2,
  Laptop,
  MessageCircle,
  Monitor,
  Pencil,
  Plus,
  Printer,
  ShieldCheck,
  ShoppingCart,
  Smartphone,
  Tablet,
  Trash2,
  Wrench,
} from 'lucide-react';
import { api, API_URL } from '../../api/api';
import { fechaCorta, hoyLima } from '../../utils/formato';
import { formatoCantidad } from '../../utils/medidas';
import { METODOS_DE_ABONO, nombreMetodo } from '../../utils/metodoPago';
import { enlaceWhatsapp } from '../../utils/vehiculos';
import { imprimirTicket } from '../../utils/imprimirTicket';
import { confirmar } from '../../utils/confirmar';
import { comprimirImagen } from '../../utils/comprimirImagen';
import { esRespaldoLavanderia } from '../../utils/textosEncargos';
import '../../components/PantallaModulo.css';
import '../../components/Recibo.css';
import '../Encargos/Encargos.css';
import './Reparaciones.css';

// Órdenes de reparación (módulo REPARACIONES, rubro Tecnología). Cada orden
// es un encargo por dentro (handlers/reparaciones.rs): el adelanto no lleva
// comprobante y al entregar se cobra el saldo en el punto de venta, con la
// boleta por el total. La clave de desbloqueo se borra al entregar.

const TIPOS_EQUIPO = [
  { valor: 'CELULAR', label: 'Celular', Icono: Smartphone },
  { valor: 'LAPTOP', label: 'Laptop', Icono: Laptop },
  { valor: 'PC', label: 'PC', Icono: Monitor },
  { valor: 'TABLET', label: 'Tablet', Icono: Tablet },
  { valor: 'IMPRESORA', label: 'Impresora', Icono: Printer },
  { valor: 'CONSOLA', label: 'Consola', Icono: Gamepad2 },
  { valor: 'OTRO', label: 'Otro', Icono: Wrench },
];
const TIPO = Object.fromEntries(TIPOS_EQUIPO.map((t) => [t.valor, t]));

const ETAPAS = {
  RECIBIDO: { label: 'Recibido', chip: 'pm-chip-aviso' },
  DIAGNOSTICO: { label: 'En diagnóstico', chip: 'pm-chip-aviso' },
  ESPERANDO_APROBACION: { label: 'Esperando aprobación', chip: 'rp-chip-espera' },
  EN_REPARACION: { label: 'En reparación', chip: 'pm-chip-aviso' },
  LISTO: { label: 'Listo para recoger', chip: 'pm-chip-ok' },
  NO_REPARADO: { label: 'No se pudo reparar', chip: 'pm-chip-mal' },
};
// Cómo se agrupan las órdenes en el taller (lo más urgente arriba).
const ORDEN_GRUPOS = ['LISTO', 'NO_REPARADO', 'ESPERANDO_APROBACION', 'EN_REPARACION', 'DIAGNOSTICO', 'RECIBIDO'];

const FILTROS = [
  { valor: 'ACTIVOS', label: 'En el taller' },
  { valor: 'ENTREGADOS', label: 'Entregados' },
  { valor: 'CANCELADOS', label: 'Cancelados' },
  { valor: 'TODOS', label: 'Todos' },
];

const GARANTIAS = [0, 7, 30, 90];

const soles = (n) => `S/ ${Number(n || 0).toFixed(2)}`;
const numeroDe = (texto) => Number(String(texto ?? '').replace(',', '.'));
const redondear2 = (n) => Math.round(n * 100) / 100;

/** "Samsung Galaxy A12" (o "Celular" si no hay marca ni modelo). */
function nombreEquipo(r) {
  const nombre = [r.marca, r.modelo].filter(Boolean).join(' ');
  return nombre || TIPO[r.tipo_equipo]?.label || 'Equipo';
}

function Etapa({ r }) {
  if (r.estado === 'ENTREGADO') return <span className="pm-chip">Entregado</span>;
  if (r.estado === 'CANCELADO') return <span className="pm-chip pm-chip-mal">Cancelado</span>;
  const e = ETAPAS[r.etapa] || { label: r.etapa, chip: '' };
  return <span className={`pm-chip ${e.chip}`}>{e.label}</span>;
}

/** Mensaje de WhatsApp según la etapa: recepción, presupuesto, listo... */
function mensajeWhatsapp(r, tienda) {
  const nombre = (r.cliente_nombre || '').split(' ')[0];
  const equipo = nombreEquipo(r);
  const pagado = redondear2((r.adelantado || 0) - (r.devuelto || 0));
  const garantia = r.garantia_dias > 0 ? `Garantía: ${r.garantia_dias} días desde la entrega.` : null;
  const lineas = (r.items || []).map((i) => `• ${i.nombre}${Number(i.cantidad) !== 1 ? ` x${formatoCantidad(i.cantidad)}` : ''}: ${soles(i.total)}`);
  if (r.etapa === 'ESPERANDO_APROBACION') {
    return [
      `Hola ${nombre} 👋`,
      `Ya revisamos tu ${equipo} (orden N° ${r.numero}) en ${tienda}.`,
      '',
      ...(r.diagnostico ? [`Diagnóstico: ${r.diagnostico}`, ''] : []),
      'Presupuesto:',
      ...lineas,
      `Total: ${soles(r.total)}`,
      ...(pagado > 0 ? [`Ya dejaste: ${soles(pagado)}`] : []),
      ...(garantia ? [garantia] : []),
      '',
      '¿Lo reparamos? Respóndenos SÍ para empezar.',
    ].join('\n');
  }
  if (r.etapa === 'LISTO') {
    return [
      `Hola ${nombre} 👋`,
      `¡Tu ${equipo} ya está listo! ✅ Orden N° ${r.numero} de ${tienda}.`,
      '',
      'Ya puedes pasar a recogerlo.',
      r.saldo > 0 ? `Saldo por pagar: ${soles(r.saldo)}` : 'Ya está todo pagado ✅',
      ...(garantia ? [garantia] : []),
      '',
      '¡Gracias por tu confianza!',
    ].join('\n');
  }
  if (r.etapa === 'NO_REPARADO') {
    return [
      `Hola ${nombre}, revisamos tu ${equipo} (orden N° ${r.numero}) en ${tienda}.`,
      `Lamentablemente no fue posible repararlo${r.diagnostico ? `: ${r.diagnostico}` : '.'}`,
      '',
      r.total > 0 ? `Revisión: ${soles(r.total)}${r.saldo < r.total ? ` (saldo ${soles(r.saldo)})` : ''}` : 'No hay nada que pagar.',
      'Puedes pasar a recogerlo cuando gustes.',
    ].join('\n');
  }
  if (r.etapa === 'EN_REPARACION') {
    return [
      `Hola ${nombre}, tu ${equipo} (orden N° ${r.numero}) ya está en reparación en ${tienda}. 🔧`,
      `Te lo tenemos para el ${fechaCorta(r.fecha_entrega)}${r.hora_entrega ? ` a las ${r.hora_entrega}` : ''}. Te avisamos por aquí cuando esté listo.`,
    ].join('\n');
  }
  return [
    `Hola ${nombre} 👋`,
    `Recibimos tu ${equipo} en ${tienda}. Orden N° ${r.numero}.`,
    '',
    `Falla: ${r.falla}`,
    ...(r.accesorios ? [`Deja: ${r.accesorios}`] : []),
    ...(r.estado_fisico ? [`Cómo llega: ${r.estado_fisico}`] : []),
    ...(pagado > 0 ? [`Adelanto: ${soles(pagado)}`] : []),
    '',
    'Te avisamos el diagnóstico y el presupuesto por aquí. Guarda tu ticket para recogerlo.',
  ].join('\n');
}

/**
 * Órdenes de reparación. El cliente deja el equipo (ticket de recepción),
 * el técnico lo revisa, se le manda el presupuesto por WhatsApp, lo aprueba,
 * se repara y se le avisa. Al entregar se cobra en el punto de venta.
 */
export default function Reparaciones({ nombreTienda, direccion, telefono, onEntregar }) {
  const [filtro, setFiltro] = useState('ACTIVOS');
  const [buscar, setBuscar] = useState('');
  const [lista, setLista] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [abierta, setAbierta] = useState(null);
  const [formulario, setFormulario] = useState(null);
  const [impresion, setImpresion] = useState(null);

  const cargar = useCallback(() => {
    api
      .reparaciones(filtro, buscar.trim())
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

  useEffect(() => {
    if (!impresion) return undefined;
    const t = setTimeout(() => imprimirTicket(), 80);
    return () => clearTimeout(t);
  }, [impresion]);

  const abrir = async (id) => {
    setError(null);
    try {
      setAbierta(await api.reparacion(id));
    } catch (e) {
      setError(e.message);
    }
  };

  const grupos = useMemo(() => {
    if (filtro !== 'ACTIVOS') return [{ clave: 'todas', titulo: null, items: lista }];
    return ORDEN_GRUPOS.map((etapa) => ({ clave: etapa, titulo: ETAPAS[etapa].label, items: lista.filter((r) => r.etapa === etapa) })).filter(
      (g) => g.items.length > 0
    );
  }, [lista, filtro]);

  const hoy = hoyLima();

  return (
    <div className="pm-layout">
      <header className="pm-cabecera">
        <div>
          <h1>
            <Smartphone size={22} /> Órdenes de reparación
          </h1>
          <p className="pm-subtitulo">
            Recibe el equipo con su falla, arma el presupuesto y mándalo por WhatsApp. El adelanto no emite comprobante: al entregar se
            cobra el saldo y sale la boleta.
          </p>
        </div>
        <div className="pm-acciones">
          <button className="pm-boton" onClick={() => setFormulario({})}>
            <Plus size={15} /> Recibir equipo
          </button>
        </div>
      </header>

      {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}

      <div className="enc-barra">
        <div className="pm-filtros" role="tablist">
          {FILTROS.map((f) => (
            <button
              key={f.valor}
              role="tab"
              aria-selected={filtro === f.valor}
              className={filtro === f.valor ? 'activo' : ''}
              onClick={() => {
                setCargando(true);
                setFiltro(f.valor);
              }}
            >
              {f.label}
            </button>
          ))}
        </div>
        <input
          className="enc-buscar"
          value={buscar}
          placeholder="Cliente, celular, N°, modelo o IMEI"
          onChange={(e) => setBuscar(e.target.value)}
          aria-label="Buscar órdenes"
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
              ? 'Ninguna orden coincide con la búsqueda.'
              : filtro === 'ACTIVOS'
                ? 'No hay equipos en el taller. Toca “Recibir equipo” cuando un cliente deje uno.'
                : 'No hay órdenes.'}
          </p>
        </div>
      ) : (
        grupos.map((g) => (
          <section key={g.clave} className="enc-grupo">
            {g.titulo && (
              <h2 className="enc-grupo-titulo">
                {g.titulo}
                <span>{g.items.length}</span>
              </h2>
            )}
            <div className="pm-tarjeta">
              {g.items.map((r) => {
                const Icono = TIPO[r.tipo_equipo]?.Icono || Wrench;
                const atrasada = r.estado !== 'ENTREGADO' && r.estado !== 'CANCELADO' && r.fecha_entrega < hoy && r.etapa !== 'LISTO';
                return (
                  <button key={r.id} className="pm-fila rp-fila" onClick={() => abrir(r.id)}>
                    <span className="rp-icono" aria-hidden="true">
                      <Icono size={18} />
                    </span>
                    <span className="pm-fila-principal">
                      <span className="pm-fila-titulo">
                        N° {r.numero} · {nombreEquipo(r)} <Etapa r={r} />
                        {r.en_garantia && (
                          <span className="pm-chip pm-chip-ok">
                            <ShieldCheck size={12} /> Garantía
                          </span>
                        )}
                      </span>
                      <span className="pm-fila-detalle" style={{ display: 'block' }}>
                        {r.cliente_nombre} · {r.falla}
                      </span>
                      <span className={`pm-fila-detalle rp-fecha${atrasada ? ' rp-atrasada' : ''}`} style={{ display: 'block' }}>
                        {r.estado === 'ENTREGADO'
                          ? `Entregado ${fechaCorta(r.fecha_entregado)}${r.garantia_hasta ? ` · garantía hasta ${fechaCorta(r.garantia_hasta)}` : ''}`
                          : `${atrasada ? 'Atrasado · ' : ''}Para el ${fechaCorta(r.fecha_entrega)}${r.tecnico ? ` · ${r.tecnico}` : ''}`}
                      </span>
                    </span>
                    <span className="pm-fila-monto">
                      <strong>{r.total > 0 ? soles(r.total) : 'Sin presupuesto'}</strong>
                      {r.estado !== 'ENTREGADO' && r.estado !== 'CANCELADO' && r.total > 0 && (
                        <small className={r.saldo > 0 ? 'enc-saldo' : 'enc-pagado'}>{r.saldo > 0 ? `Saldo ${soles(r.saldo)}` : 'Pagado'}</small>
                      )}
                    </span>
                  </button>
                );
              })}
            </div>
          </section>
        ))
      )}

      {formulario && (
        <FormularioReparacion
          orden={formulario.orden || null}
          onCerrar={() => setFormulario(null)}
          onGuardado={(r, nueva) => {
            setFormulario(null);
            setAbierta(r);
            cargar();
            // Al recibir el equipo sale el ticket para el cliente.
            if (nueva) setImpresion({ tipo: 'CLIENTE', orden: r, n: Date.now() });
          }}
        />
      )}

      {abierta && !formulario && (
        <DetalleReparacion
          orden={abierta}
          nombreTienda={nombreTienda}
          onCambio={(r) => {
            setAbierta(r);
            cargar();
          }}
          onCerrar={() => setAbierta(null)}
          onEditar={() => setFormulario({ orden: abierta })}
          onImprimir={(tipo) => setImpresion({ tipo, orden: abierta, n: Date.now() })}
          onEntregar={onEntregar}
        />
      )}

      {impresion &&
        createPortal(
          <ReparacionImprimible
            key={impresion.n}
            tipo={impresion.tipo}
            orden={impresion.orden}
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
// Recibir equipo / editar
// ---------------------------------------------------------------------

const lineaDeItem = (i) => ({
  producto_id: i.producto_id,
  nombre: i.nombre,
  libre: !!i.libre,
  cantidad: String(formatoCantidad(i.cantidad)),
  precio: String(i.precio_unitario),
  detalle: i.detalle || '',
});

function FormularioReparacion({ orden, onCerrar, onGuardado }) {
  const editando = !!orden;
  const [datos, setDatos] = useState(() => ({
    cliente: orden?.cliente_id
      ? { id: orden.cliente_id, nombre_razon_social: orden.cliente_nombre, numero_documento: orden.cliente_documento }
      : null,
    cliente_nombre: orden && !orden.cliente_id ? orden.cliente_nombre : '',
    cliente_telefono: orden?.cliente_telefono || '',
    tipo_equipo: orden?.tipo_equipo || 'CELULAR',
    marca: orden?.marca || '',
    modelo: orden?.modelo || '',
    serie: orden?.serie || '',
    color: orden?.color || '',
    accesorios: orden?.accesorios || '',
    clave: orden?.clave || '',
    estado_fisico: orden?.estado_fisico || '',
    falla: orden?.falla || '',
    diagnostico: orden?.diagnostico || '',
    tecnico: orden?.tecnico || '',
    garantia_dias: String(orden?.garantia_dias ?? 0),
    fecha_entrega: orden?.fecha_entrega || hoyLima(2),
    hora_entrega: orden?.hora_entrega || '',
    notas: orden?.notas || '',
    items: (orden?.items || []).map(lineaDeItem),
    adelanto: '',
    adelanto_metodo: 'EFECTIVO',
  }));
  const [productos, setProductos] = useState([]);
  const [tecnicos, setTecnicos] = useState([]);
  const [buscarProducto, setBuscarProducto] = useState('');
  const [buscarCliente, setBuscarCliente] = useState('');
  const [clientes, setClientes] = useState([]);
  const [garantiaPrevia, setGarantiaPrevia] = useState([]);
  const [verClave, setVerClave] = useState(false);
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const cambiar = (campo, valor) => setDatos((d) => ({ ...d, [campo]: valor }));

  useEffect(() => {
    api
      .productos()
      .then((lista) => setProductos((lista || []).filter((p) => !esRespaldoLavanderia(p))))
      .catch(() => {});
    api
      .reparacionTecnicos()
      .then((l) => setTecnicos(l || []))
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

  // Si el equipo ya se reparó aquí (mismo N° de serie o IMEI), se avisa: puede venir en garantía.
  useEffect(() => {
    const serie = datos.serie.replace(/[^0-9a-z]/gi, '');
    if (serie.length < 6) return undefined;
    let vigente = true;
    const t = setTimeout(() => {
      api
        .reparacionGarantia(serie, orden?.id)
        .then((l) => vigente && setGarantiaPrevia(l || []))
        .catch(() => vigente && setGarantiaPrevia([]));
    }, 400);
    return () => {
      vigente = false;
      clearTimeout(t);
    };
  }, [datos.serie, orden?.id]);
  const previas = datos.serie.replace(/[^0-9a-z]/gi, '').length >= 6 ? garantiaPrevia : [];

  // DNI (8) o RUC (11) que aún no es cliente: se consulta RENIEC/SUNAT para registrarlo.
  const documentoBuscado = useMemo(() => {
    const q = buscarCliente.trim();
    if (/^\d{8}$/.test(q)) return { tipo: 'DNI', numero: q };
    if (/^\d{11}$/.test(q)) return { tipo: 'RUC', numero: q };
    return null;
  }, [buscarCliente]);
  const [consultas, setConsultas] = useState({});
  useEffect(() => {
    if (!documentoBuscado || consultas[documentoBuscado.numero]) return undefined;
    const { tipo, numero } = documentoBuscado;
    let cancelado = false;
    api
      .documentoConsultar(tipo, numero)
      .then((r) => {
        if (cancelado) return;
        const encontrado = r?.existe === true && r?.nombre;
        setConsultas((c) => ({ ...c, [numero]: encontrado ? { nombre: r.nombre, direccion: r.direccion || null } : { nombre: null } }));
      })
      .catch(() => !cancelado && setConsultas((c) => ({ ...c, [numero]: { nombre: null } })));
    return () => {
      cancelado = true;
    };
  }, [documentoBuscado, consultas]);
  const yaEsCliente = !!documentoBuscado && clientes.some((c) => c.numero_documento === documentoBuscado.numero);
  const consultado = documentoBuscado && !yaEsCliente ? consultas[documentoBuscado.numero] : null;

  const elegirCliente = (c) => {
    setDatos((d) => ({ ...d, cliente: c, cliente_nombre: '', cliente_telefono: d.cliente_telefono || c.telefono || '' }));
    setBuscarCliente('');
    setClientes([]);
  };
  const registrarConDocumento = async (nombre, direccionCliente) => {
    setError(null);
    setOcupado(true);
    try {
      const nuevo = await api.clienteCrear({
        tipo_documento: documentoBuscado.tipo,
        numero_documento: documentoBuscado.numero,
        nombre_razon_social: nombre.trim(),
        telefono: datos.cliente_telefono.trim() || null,
        direccion: direccionCliente || null,
      });
      elegirCliente(nuevo);
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  const encontrados = useMemo(() => {
    const q = buscarProducto.trim().toLowerCase();
    if (q.length < 2) return [];
    return productos.filter((p) => p.nombre.toLowerCase().includes(q) || String(p.codigo || '').toLowerCase().includes(q)).slice(0, 8);
  }, [productos, buscarProducto]);

  const agregar = (p) => {
    setBuscarProducto('');
    setDatos((d) => ({
      ...d,
      items: [...d.items, { producto_id: p.id, nombre: p.nombre, libre: false, cantidad: '1', precio: String(p.precio), detalle: '', stock: p.stock }],
    }));
  };
  const agregarManoDeObra = () =>
    setDatos((d) => ({ ...d, items: [...d.items, { producto_id: null, libre: true, nombre: '', cantidad: '1', precio: '', detalle: '' }] }));
  const cambiarItem = (idx, campo, valor) =>
    setDatos((d) => ({ ...d, items: d.items.map((i, n) => (n === idx ? { ...i, [campo]: valor } : i)) }));
  const quitarItem = (idx) => setDatos((d) => ({ ...d, items: d.items.filter((_, n) => n !== idx) }));

  const total = redondear2(datos.items.reduce((s, i) => s + redondear2((numeroDe(i.precio) || 0) * (numeroDe(i.cantidad) || 0)), 0));
  const adelanto = numeroDe(datos.adelanto) || 0;

  const guardar = async () => {
    setError(null);
    if (!datos.cliente && !datos.cliente_nombre.trim()) return setError('Elige un cliente o escribe su nombre.');
    if (!datos.falla.trim()) return setError('Escribe qué falla tiene el equipo (lo que cuenta el cliente).');
    if (datos.items.some((i) => i.libre && !i.nombre.trim())) return setError('Escribe qué trabajo es en cada línea de mano de obra.');
    if (datos.items.some((i) => !(numeroDe(i.cantidad) > 0) || !(numeroDe(i.precio) >= 0))) return setError('Revisa las cantidades y los precios.');
    if (!editando && adelanto > total + 0.005)
      return setError(total > 0 ? 'El adelanto no puede ser mayor que el presupuesto.' : 'Para recibir un adelanto, pon primero el presupuesto.');
    const garantia = Math.round(numeroDe(datos.garantia_dias) || 0);
    if (garantia < 0 || garantia > 730) return setError('La garantía va de 0 a 730 días.');
    const cuerpo = {
      cliente_id: datos.cliente?.id || null,
      cliente_nombre: datos.cliente ? null : datos.cliente_nombre.trim(),
      cliente_telefono: datos.cliente_telefono.trim() || null,
      fecha_entrega: datos.fecha_entrega,
      hora_entrega: datos.hora_entrega || null,
      notas: datos.notas.trim() || null,
      equipo: {
        tipo_equipo: datos.tipo_equipo,
        marca: datos.marca,
        modelo: datos.modelo,
        serie: datos.serie,
        color: datos.color,
        accesorios: datos.accesorios,
        clave: datos.clave,
        estado_fisico: datos.estado_fisico,
        falla: datos.falla,
      },
      diagnostico: datos.diagnostico.trim() || null,
      tecnico: datos.tecnico.trim() || null,
      garantia_dias: garantia,
      items: datos.items.map((i) => ({
        producto_id: i.libre ? (i.producto_id ?? null) : i.producto_id,
        ...(i.libre ? { nombre: i.nombre.trim(), cobro: 'SERVICIO' } : {}),
        cantidad: numeroDe(i.cantidad),
        precio_unitario: numeroDe(i.precio),
        detalle: i.detalle.trim() || null,
      })),
      ...(!editando && adelanto > 0 ? { adelanto: { monto: adelanto, metodo_pago: datos.adelanto_metodo } } : {}),
    };
    setOcupado(true);
    try {
      const r = editando ? await api.reparacionActualizar(orden.id, cuerpo) : await api.reparacionCrear(cuerpo);
      onGuardado(r, !editando);
    } catch (e) {
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && !ocupado && onCerrar()}>
      <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label={editando ? `Editar orden N° ${orden.numero}` : 'Recibir equipo'}>
        <div className="pm-modal-cabecera">
          <h2>{editando ? `Orden N° ${orden.numero}: diagnóstico y presupuesto` : 'Recibir equipo'}</h2>
          <p>
            {editando
              ? 'Anota lo que encontró el técnico y arma el presupuesto con repuestos del stock y mano de obra.'
              : 'Anota el equipo, cómo llega y la falla. El presupuesto lo puedes poner ahora o después del diagnóstico.'}
          </p>
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
                <span>Buscar cliente o escribir su DNI / RUC (opcional)</span>
                <input
                  value={buscarCliente}
                  placeholder="Nombre, DNI o RUC"
                  inputMode={/^\d+$/.test(buscarCliente.trim()) ? 'numeric' : undefined}
                  onChange={(e) => setBuscarCliente(e.target.value)}
                />
              </label>
              {documentoBuscado && !yaEsCliente && !consultado && (
                <p className="pm-subtitulo pm-campo-ancho">Consultando {documentoBuscado.tipo === 'RUC' ? 'SUNAT' : 'RENIEC'}…</p>
              )}
              {consultado?.nombre && (
                <div className="pm-campo-ancho enc-doc-encontrado">
                  <span>
                    <strong>{consultado.nombre}</strong>
                    <small>
                      {' '}
                      · {documentoBuscado.tipo} {documentoBuscado.numero}
                    </small>
                  </span>
                  <button type="button" className="pm-boton" disabled={ocupado} onClick={() => registrarConDocumento(consultado.nombre, consultado.direccion)}>
                    Usar este cliente
                  </button>
                </div>
              )}
              {consultado && !consultado.nombre && (
                <div className="pm-campo-ancho enc-doc-encontrado">
                  <span className="pm-subtitulo">
                    No se encontró el {documentoBuscado.tipo} {documentoBuscado.numero}. Escribe su nombre abajo
                    {datos.cliente_nombre.trim() ? '' : ' y podrás registrarlo con este documento'}.
                  </span>
                  {datos.cliente_nombre.trim() && (
                    <button type="button" className="pm-boton-secundario" disabled={ocupado} onClick={() => registrarConDocumento(datos.cliente_nombre)}>
                      Registrar con {documentoBuscado.tipo}
                    </button>
                  )}
                </div>
              )}
              {buscarCliente.trim().length >= 2 && clientes.length > 0 && (
                <div className="pm-campo-ancho">
                  {clientes.map((c) => (
                    <button key={c.id} type="button" className="pm-fila" onClick={() => elegirCliente(c)}>
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

          <p className="pm-seccion">Equipo</p>
          <div className="pm-opciones rp-tipos" role="radiogroup" aria-label="Tipo de equipo">
            {TIPOS_EQUIPO.map(({ valor, label, Icono }) => (
              <button
                key={valor}
                type="button"
                role="radio"
                aria-checked={datos.tipo_equipo === valor}
                className={datos.tipo_equipo === valor ? 'activo' : ''}
                onClick={() => cambiar('tipo_equipo', valor)}
              >
                <Icono size={14} /> {label}
              </button>
            ))}
          </div>
          <div className="pm-campos">
            <label className="pm-campo">
              <span>Marca</span>
              <input value={datos.marca} maxLength={60} placeholder="Samsung, HP, Epson..." onChange={(e) => cambiar('marca', e.target.value)} />
            </label>
            <label className="pm-campo">
              <span>Modelo</span>
              <input value={datos.modelo} maxLength={60} placeholder="Galaxy A12, Pavilion 15..." onChange={(e) => cambiar('modelo', e.target.value)} />
            </label>
            <label className="pm-campo">
              <span>{datos.tipo_equipo === 'CELULAR' ? 'IMEI o N° de serie' : 'N° de serie'} (opcional)</span>
              <input value={datos.serie} maxLength={40} onChange={(e) => cambiar('serie', e.target.value)} />
              {datos.tipo_equipo === 'CELULAR' && <small>En el celular: marca *#06# para ver el IMEI.</small>}
            </label>
            <label className="pm-campo">
              <span>Color</span>
              <input value={datos.color} maxLength={30} onChange={(e) => cambiar('color', e.target.value)} />
            </label>
            {previas.length > 0 && (
              <div className={`pm-campo-ancho rp-aviso-garantia${previas.some((p) => p.en_garantia) ? ' vigente' : ''}`}>
                <ShieldCheck size={16} />
                <span>
                  {previas.some((p) => p.en_garantia) ? <strong>Este equipo está en garantía. </strong> : <strong>Este equipo ya vino antes. </strong>}
                  {previas
                    .slice(0, 3)
                    .map(
                      (p) =>
                        `Orden N° ${p.numero} (${fechaCorta(p.fecha_entregado)}${p.garantia_hasta ? `, garantía hasta ${fechaCorta(p.garantia_hasta)}` : ', sin garantía'})`
                    )
                    .join(' · ')}
                </span>
              </div>
            )}
            <label className="pm-campo pm-campo-ancho">
              <span>Qué deja con el equipo</span>
              <input
                value={datos.accesorios}
                maxLength={200}
                placeholder="Cargador, funda, chip, memoria, mouse..."
                onChange={(e) => cambiar('accesorios', e.target.value)}
              />
            </label>
            <label className="pm-campo pm-campo-ancho">
              <span>Cómo llega</span>
              <input
                value={datos.estado_fisico}
                maxLength={300}
                placeholder="Pantalla rota, rayones, golpe en una esquina, no enciende..."
                onChange={(e) => cambiar('estado_fisico', e.target.value)}
              />
            </label>
            <label className="pm-campo pm-campo-ancho">
              <span>Falla (lo que cuenta el cliente)</span>
              <textarea rows={2} maxLength={500} value={datos.falla} placeholder="No carga, se apaga solo, no imprime..." onChange={(e) => cambiar('falla', e.target.value)} />
            </label>
            <label className="pm-campo">
              <span>Clave o patrón de desbloqueo (opcional)</span>
              <div className="rp-clave">
                <input
                  type={verClave ? 'text' : 'password'}
                  value={datos.clave}
                  maxLength={60}
                  autoComplete="off"
                  onChange={(e) => cambiar('clave', e.target.value)}
                />
                <button type="button" className="pm-boton-secundario" onClick={() => setVerClave((v) => !v)} aria-label={verClave ? 'Ocultar clave' : 'Ver clave'}>
                  {verClave ? <EyeOff size={14} /> : <Eye size={14} />}
                </button>
              </div>
              <small>Solo si el cliente la da. Se borra sola al entregar el equipo.</small>
            </label>
          </div>

          <p className="pm-seccion">Entrega</p>
          <div className="pm-campos">
            <label className="pm-campo">
              <span>Lo tienes para el</span>
              <input type="date" min={editando ? undefined : hoyLima()} value={datos.fecha_entrega} onChange={(e) => cambiar('fecha_entrega', e.target.value)} />
            </label>
            <label className="pm-campo">
              <span>Hora (opcional)</span>
              <input type="time" value={datos.hora_entrega} onChange={(e) => cambiar('hora_entrega', e.target.value)} />
            </label>
          </div>

          <p className="pm-seccion">Diagnóstico y técnico</p>
          <div className="pm-campos">
            <label className="pm-campo pm-campo-ancho">
              <span>Diagnóstico (lo que encontró el técnico)</span>
              <textarea
                rows={2}
                maxLength={1000}
                value={datos.diagnostico}
                placeholder="Display dañado, la placa está bien. Pin de carga sulfatado..."
                onChange={(e) => cambiar('diagnostico', e.target.value)}
              />
            </label>
            <label className="pm-campo">
              <span>Técnico</span>
              <input value={datos.tecnico} maxLength={60} onChange={(e) => cambiar('tecnico', e.target.value)} />
            </label>
            {tecnicos.length > 0 && (
              <div className="pm-campo pm-campo-ancho">
                <div className="pm-opciones">
                  {tecnicos.map((n) => (
                    <button key={n} type="button" className={datos.tecnico === n ? 'activo' : ''} onClick={() => cambiar('tecnico', n)}>
                      {n}
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>

          <p className="pm-seccion">Presupuesto</p>
          {datos.items.length === 0 && <p className="pm-vacio">Agrega los repuestos del stock y la mano de obra. Si aún no lo sabes, déjalo para después del diagnóstico.</p>}
          {datos.items.map((i, idx) => (
            <div key={`${i.producto_id}-${idx}`} className="enc-item">
              <div className="enc-item-cabecera">
                {i.libre ? (
                  <label className="pm-campo enc-item-prenda">
                    <span>Mano de obra o servicio</span>
                    <input
                      value={i.nombre}
                      maxLength={120}
                      placeholder="Cambio de pantalla, limpieza de placa, formateo..."
                      autoFocus={!i.nombre && idx === datos.items.length - 1}
                      onChange={(e) => cambiarItem(idx, 'nombre', e.target.value)}
                    />
                  </label>
                ) : (
                  <strong>
                    {i.nombre}
                    {i.stock != null && i.stock <= 0 && <small className="rp-sin-stock"> · sin stock</small>}
                  </strong>
                )}
                <button type="button" className="pm-boton-secundario" onClick={() => quitarItem(idx)} aria-label={`Quitar ${i.nombre || 'línea'}`}>
                  <Trash2 size={14} />
                </button>
              </div>
              <div className="enc-item-campos">
                <label className="pm-campo">
                  <span>Cantidad</span>
                  <input inputMode="decimal" value={i.cantidad} onChange={(e) => cambiarItem(idx, 'cantidad', e.target.value)} />
                </label>
                <label className="pm-campo">
                  <span>Precio (c/u)</span>
                  <input inputMode="decimal" value={i.precio} placeholder="0.00" onChange={(e) => cambiarItem(idx, 'precio', e.target.value)} />
                </label>
                <label className="pm-campo enc-item-detalle">
                  <span>Detalle (opcional)</span>
                  <input value={i.detalle} maxLength={500} placeholder={i.libre ? 'Incluye pegado y prueba' : 'Original, genérico, color...'} onChange={(e) => cambiarItem(idx, 'detalle', e.target.value)} />
                </label>
              </div>
            </div>
          ))}
          <button type="button" className="pm-boton-secundario enc-prenda-nueva" onClick={agregarManoDeObra}>
            <Plus size={14} /> Mano de obra
          </button>
          <div className="pm-campo" style={{ marginTop: 8 }}>
            <input value={buscarProducto} placeholder="Agregar repuesto: busca por nombre o código" onChange={(e) => setBuscarProducto(e.target.value)} />
          </div>
          {encontrados.map((p) => (
            <button key={p.id} type="button" className="pm-fila" onClick={() => agregar(p)}>
              <span className="pm-fila-principal">
                <span className="pm-fila-titulo">{p.nombre}</span>
                <span className="pm-fila-detalle" style={{ display: 'block' }}>
                  {p.codigo}
                  {p.controla_stock !== false && p.stock != null ? ` · stock ${formatoCantidad(p.stock)}` : ''}
                </span>
              </span>
              <span className="pm-fila-monto">
                <strong>{soles(p.precio)}</strong>
              </span>
            </button>
          ))}
          <div className="pm-total">
            <span>Total del presupuesto</span>
            <strong>{soles(total)}</strong>
          </div>

          <p className="pm-seccion">Garantía</p>
          <div className="pm-campos">
            <div className="pm-campo pm-campo-ancho">
              <div className="pm-opciones">
                {GARANTIAS.map((g) => (
                  <button key={g} type="button" className={Number(datos.garantia_dias) === g ? 'activo' : ''} onClick={() => cambiar('garantia_dias', String(g))}>
                    {g === 0 ? 'Sin garantía' : `${g} días`}
                  </button>
                ))}
              </div>
            </div>
            <label className="pm-campo">
              <span>Otro (días)</span>
              <input inputMode="numeric" value={datos.garantia_dias} onChange={(e) => cambiar('garantia_dias', e.target.value.replace(/\D/g, '').slice(0, 3))} />
              <small>Corre desde el día que se entrega.</small>
            </label>
          </div>

          <p className="pm-seccion">Notas</p>
          <div className="pm-campos">
            <label className="pm-campo pm-campo-ancho">
              <textarea rows={2} maxLength={1000} value={datos.notas} onChange={(e) => cambiar('notas', e.target.value)} aria-label="Notas" />
            </label>
          </div>

          {!editando && total > 0 && (
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
              {adelanto > 0 && (
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
            {ocupado ? 'Guardando...' : editando ? 'Guardar cambios' : 'Recibir equipo e imprimir ticket'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Detalle
// ---------------------------------------------------------------------

/** Los pasos que siguen desde cada etapa (el primero es el principal). */
const SIGUIENTES = {
  RECIBIDO: [['DIAGNOSTICO', 'Empezar diagnóstico']],
  DIAGNOSTICO: [
    ['ESPERANDO_APROBACION', 'Presupuesto listo'],
    ['NO_REPARADO', 'No tiene arreglo'],
  ],
  ESPERANDO_APROBACION: [
    ['EN_REPARACION', 'El cliente aprobó'],
    ['NO_REPARADO', 'No aprobó'],
  ],
  EN_REPARACION: [
    ['LISTO', 'Marcar listo'],
    ['NO_REPARADO', 'No se pudo reparar'],
  ],
  LISTO: [['EN_REPARACION', 'Volver a reparación']],
  NO_REPARADO: [['EN_REPARACION', 'Volver a reparación']],
};

function DetalleReparacion({ orden, nombreTienda, onCambio, onCerrar, onEditar, onImprimir, onEntregar }) {
  const r = orden;
  const activo = !['ENTREGADO', 'CANCELADO'].includes(r.estado);
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const [adelanto, setAdelanto] = useState(null);
  const [cancelando, setCancelando] = useState(null);
  const [verClave, setVerClave] = useState(false);
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
  // Adelanto, cancelar y foto van por las rutas de encargos: luego se relee la orden completa.
  const conRelectura = (accion) => ejecutar(async () => {
    await accion();
    return api.reparacion(r.id);
  });

  const cambiarEtapa = async (etapa) => {
    if (etapa === 'ESPERANDO_APROBACION' && !(r.total > 0)) {
      setError('Pon primero el presupuesto (Editar → Presupuesto) para mandárselo al cliente.');
      return;
    }
    await ejecutar(() => api.reparacionEtapa(r.id, etapa));
  };

  const registrarAdelanto = async () => {
    const monto = numeroDe(adelanto.monto);
    if (!(monto > 0)) return setError('Escribe el monto del adelanto.');
    if (await conRelectura(() => api.encargoAdelanto(r.id, { monto, metodo_pago: adelanto.metodo }))) setAdelanto(null);
  };

  const cancelar = async () => {
    if (!cancelando.motivo.trim()) return setError('Escribe por qué se cancela.');
    const ok = await conRelectura(() =>
      api.encargoCancelar(r.id, { motivo: cancelando.motivo.trim(), devolver: cancelando.devolver, metodo_devolucion: cancelando.metodo })
    );
    if (ok) setCancelando(null);
  };

  const subirFoto = async (archivo) => {
    if (!archivo) return;
    await conRelectura(async () => api.encargoSubirFoto(r.id, await comprimirImagen(archivo, { ladoMaximo: 1600 })));
    if (archivoRef.current) archivoRef.current.value = '';
  };

  const entregar = async () => {
    if (
      r.etapa !== 'LISTO' &&
      r.etapa !== 'NO_REPARADO' &&
      !(await confirmar({
        titulo: `La orden N° ${r.numero} aún no está lista`,
        mensaje: '¿La entregas igual? Se abre el punto de venta para cobrar el saldo.',
        textoConfirmar: 'Entregar igual',
        tipo: 'normal',
        icono: 'aviso',
      }))
    )
      return;
    onEntregar(r);
  };

  const devolverSinCobro = async () => {
    const ok = await confirmar({
      titulo: `¿Devolver el equipo de la orden N° ${r.numero}?`,
      mensaje: 'Se entrega sin cobrar nada y sin comprobante (no se reparó y no se cobra la revisión).',
      textoConfirmar: 'Devolver sin cobro',
      tipo: 'normal',
      icono: 'aviso',
    });
    if (ok) await ejecutar(() => api.reparacionDevolver(r.id));
  };

  const pagado = redondear2(r.adelantado - r.devuelto);
  const whatsapp = enlaceWhatsapp(r.cliente_telefono, mensajeWhatsapp(r, nombreTienda || 'el taller'));
  const textoWhatsapp =
    r.etapa === 'ESPERANDO_APROBACION'
      ? 'Enviar presupuesto'
      : r.etapa === 'LISTO'
        ? 'Avisar que está listo'
        : r.etapa === 'NO_REPARADO'
          ? 'Avisar que no se reparó'
          : r.etapa === 'EN_REPARACION'
            ? 'Avisar que está en reparación'
            : 'Enviar orden por WhatsApp';
  const Icono = TIPO[r.tipo_equipo]?.Icono || Wrench;
  const siguientes = activo ? SIGUIENTES[r.etapa] || [] : [];
  const sinNadaQueCobrar = !(r.total > 0.005) && !(pagado > 0.005);

  return (
    <div className="pm-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar()}>
      <div className="pm-modal pm-modal-ancho" role="dialog" aria-modal="true" aria-label={`Orden ${r.numero}`}>
        <div className="pm-modal-cabecera">
          <h2>
            Orden N° {r.numero} · {nombreEquipo(r)} <Etapa r={r} />
          </h2>
          <p>
            {r.cliente_nombre}
            {r.cliente_telefono ? ` · ${r.cliente_telefono}` : ''} · recibido {fechaCorta(r.fecha_creacion)}
            {r.estado === 'ENTREGADO'
              ? ` · entregado ${fechaCorta(r.fecha_entregado)}`
              : ` · para el ${fechaCorta(r.fecha_entrega)}${r.hora_entrega ? ` ${r.hora_entrega}` : ''}`}
            {r.folio_venta ? ` · venta ${r.folio_venta}` : ''}
          </p>
        </div>
        <div className="pm-modal-cuerpo">
          {error && <p className="pm-mensaje pm-mensaje-error">{error}</p>}
          {r.estado === 'CANCELADO' && r.motivo_cancelacion && <p className="pm-mensaje pm-mensaje-error">Cancelado: {r.motivo_cancelacion}</p>}
          {r.estado === 'ENTREGADO' && r.garantia_hasta && (
            <p className={`pm-mensaje ${r.en_garantia ? 'pm-mensaje-exito' : 'pm-mensaje-aviso'}`}>
              <ShieldCheck size={14} /> {r.en_garantia ? 'En garantía' : 'Garantía vencida'}: {r.garantia_dias} días, hasta el {fechaCorta(r.garantia_hasta)}.
            </p>
          )}

          {siguientes.length > 0 && (
            <div className="rp-pasos" aria-label="Siguiente paso">
              {siguientes.map(([etapa, texto], i) => (
                <button key={etapa} className={i === 0 ? 'pm-boton' : 'pm-boton-secundario'} onClick={() => cambiarEtapa(etapa)} disabled={ocupado}>
                  {texto}
                </button>
              ))}
              {whatsapp && (
                <a className="pm-boton-secundario rp-whatsapp" href={whatsapp} target="_blank" rel="noreferrer">
                  <MessageCircle size={15} /> {textoWhatsapp}
                </a>
              )}
            </div>
          )}

          <div className="enc-detalle">
            <div>
              <div className="rp-equipo">
                <span className="rp-icono rp-icono-grande" aria-hidden="true">
                  <Icono size={22} />
                </span>
                <dl className="pm-pares">
                  <dt>Equipo</dt>
                  <dd>
                    {TIPO[r.tipo_equipo]?.label || r.tipo_equipo}
                    {r.marca || r.modelo ? ` · ${[r.marca, r.modelo].filter(Boolean).join(' ')}` : ''}
                    {r.color ? ` · ${r.color}` : ''}
                  </dd>
                  {r.serie && (
                    <>
                      <dt>{r.tipo_equipo === 'CELULAR' ? 'IMEI / serie' : 'N° de serie'}</dt>
                      <dd className="rp-serie">{r.serie}</dd>
                    </>
                  )}
                  {r.accesorios && (
                    <>
                      <dt>Deja</dt>
                      <dd>{r.accesorios}</dd>
                    </>
                  )}
                  {r.estado_fisico && (
                    <>
                      <dt>Cómo llega</dt>
                      <dd>{r.estado_fisico}</dd>
                    </>
                  )}
                  {r.clave && (
                    <>
                      <dt>Clave</dt>
                      <dd>
                        <span className="rp-serie">{verClave ? r.clave : '••••••'}</span>{' '}
                        <button type="button" className="rp-ver" onClick={() => setVerClave((v) => !v)}>
                          {verClave ? 'Ocultar' : 'Ver'}
                        </button>
                      </dd>
                    </>
                  )}
                  <dt>Falla</dt>
                  <dd>{r.falla}</dd>
                  {r.diagnostico && (
                    <>
                      <dt>Diagnóstico</dt>
                      <dd>{r.diagnostico}</dd>
                    </>
                  )}
                  {r.tecnico && (
                    <>
                      <dt>Técnico</dt>
                      <dd>{r.tecnico}</dd>
                    </>
                  )}
                  <dt>Garantía</dt>
                  <dd>{r.garantia_dias > 0 ? `${r.garantia_dias} días desde la entrega` : 'Sin garantía'}</dd>
                  {r.notas && (
                    <>
                      <dt>Notas</dt>
                      <dd>{r.notas}</dd>
                    </>
                  )}
                </dl>
              </div>

              <p className="pm-seccion">Presupuesto</p>
              {r.items.length === 0 ? (
                <p className="pm-vacio">{activo ? 'Todavía sin presupuesto. Toca “Editar” para agregar repuestos y mano de obra.' : 'Sin presupuesto.'}</p>
              ) : (
                <table className="pm-lineas">
                  <tbody>
                    {r.items.map((i) => (
                      <tr key={i.id}>
                        <td>
                          {formatoCantidad(i.cantidad)} × {i.nombre}
                          {i.detalle && <small>{i.detalle}</small>}
                        </td>
                        <td className="pm-num">{soles(i.total)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
              <div className="enc-montos">
                <div>
                  <span>Total</span>
                  <strong>{soles(r.total)}</strong>
                </div>
                <div>
                  <span>Adelantado</span>
                  <strong>{soles(pagado)}</strong>
                </div>
                {activo && (
                  <div className="enc-montos-saldo">
                    <span>Saldo al entregar</span>
                    <strong>{soles(r.saldo)}</strong>
                  </div>
                )}
                {r.estado === 'ENTREGADO' && r.saldo > 0 && (
                  <div className={r.a_cuenta ? 'enc-montos-saldo' : 'enc-montos-pagado'}>
                    <span>{r.a_cuenta ? 'Quedó a cuenta' : 'Cobrado al entregar'}</span>
                    <strong>{soles(r.saldo)}</strong>
                  </div>
                )}
              </div>
            </div>

            <div className="enc-foto">
              {r.foto_url ? (
                <a href={`${API_URL}${r.foto_url}`} target="_blank" rel="noreferrer">
                  <img src={`${API_URL}${r.foto_url}`} alt="Foto del equipo al recibirlo" />
                </a>
              ) : (
                <div className="enc-foto-vacia">
                  <Camera size={26} />
                  <span>Sin foto del equipo</span>
                </div>
              )}
              {activo && (
                <div className="enc-foto-acciones">
                  <input ref={archivoRef} type="file" accept="image/*" hidden onChange={(e) => subirFoto(e.target.files?.[0])} />
                  <button className="pm-boton-secundario" onClick={() => archivoRef.current?.click()} disabled={ocupado}>
                    <Camera size={14} /> {r.foto_url ? 'Cambiar foto' : 'Foto de cómo llega'}
                  </button>
                  {r.foto_url && (
                    <button className="pm-boton-secundario" onClick={() => conRelectura(() => api.encargoQuitarFoto(r.id))} disabled={ocupado}>
                      Quitar
                    </button>
                  )}
                </div>
              )}
            </div>
          </div>

          <p className="pm-seccion">Adelantos</p>
          {(r.pagos || []).length === 0 ? (
            <p className="pm-vacio">Todavía no dejó adelanto.</p>
          ) : (
            <table className="pm-lineas">
              <tbody>
                {r.pagos.map((p) => (
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
          {activo && r.saldo > 0 && !adelanto && (
            <button className="pm-boton-secundario" style={{ marginTop: 8 }} onClick={() => setAdelanto({ monto: '', metodo: 'EFECTIVO' })}>
              <Plus size={14} /> Registrar adelanto
            </button>
          )}
          {adelanto && (
            <div className="enc-recuadro">
              <div className="pm-campos">
                <label className="pm-campo">
                  <span>Monto (falta {soles(r.saldo)})</span>
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
                </>
              )}
              <div className="enc-recuadro-pie">
                <button className="pm-boton-secundario" onClick={() => setCancelando(null)}>
                  Volver
                </button>
                <button className="pm-boton-peligro" onClick={cancelar} disabled={ocupado}>
                  Cancelar orden
                </button>
              </div>
            </div>
          )}
        </div>
        <div className="pm-modal-pie enc-pie">
          {activo && !cancelando && (
            <button className="pm-boton-peligro" onClick={() => setCancelando({ motivo: '', devolver: pagado > 0, metodo: 'EFECTIVO' })} disabled={ocupado}>
              Cancelar orden
            </button>
          )}
          {activo && (
            <button className="pm-boton-secundario" onClick={onEditar} disabled={ocupado}>
              <Pencil size={14} /> Editar
            </button>
          )}
          <button className="pm-boton-secundario" onClick={() => onImprimir('TALLER')}>
            <ClipboardList size={15} /> Orden de trabajo
          </button>
          <button className="pm-boton-secundario" onClick={() => onImprimir('CLIENTE')}>
            <Printer size={15} /> Ticket del cliente
          </button>
          {whatsapp && activo && siguientes.length === 0 && (
            <a className="pm-boton-secundario" href={whatsapp} target="_blank" rel="noreferrer" style={{ textDecoration: 'none' }}>
              <MessageCircle size={15} /> {textoWhatsapp}
            </a>
          )}
          <button className="pm-boton-secundario" onClick={onCerrar}>
            Cerrar
          </button>
          {activo && sinNadaQueCobrar && (r.etapa === 'NO_REPARADO' || r.etapa === 'LISTO') ? (
            <button className="pm-boton" onClick={devolverSinCobro} disabled={ocupado}>
              Devolver sin cobro
            </button>
          ) : (
            activo && (
              <button className="pm-boton" onClick={entregar} disabled={ocupado}>
                <ShoppingCart size={15} /> {r.saldo > 0 ? `Entregar y cobrar ${soles(r.saldo)}` : 'Entregar'}
              </button>
            )
          )}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Tickets: recepción (cliente) y orden de trabajo (técnico)
// ---------------------------------------------------------------------

function Fila({ etiqueta, valor }) {
  if (!valor) return null;
  return (
    <div className="recibo-fila-meta">
      <span>{etiqueta}</span>
      <span>{valor}</span>
    </div>
  );
}

/**
 * CLIENTE: ticket de recepción, con el equipo, la falla, el presupuesto, lo
 * adelantado y el saldo (no es comprobante de pago). TALLER: para el
 * técnico, sin precios, con la clave y el diagnóstico.
 */
function ReparacionImprimible({ tipo, orden: r, nombreTienda, direccion, telefono }) {
  const taller = tipo === 'TALLER';
  const pagado = redondear2(r.adelantado - r.devuelto);
  return (
    <div className="recibo-imprimible">
      {!taller && (
        <>
          <div className="recibo-centro recibo-nombre-tienda">{nombreTienda}</div>
          {direccion && <div className="recibo-centro recibo-dato-tienda">{direccion}</div>}
          {telefono && <div className="recibo-centro recibo-dato-tienda">Tel: {telefono}</div>}
          <div className="recibo-linea"></div>
        </>
      )}
      <div className="recibo-centro recibo-comprobante-tipo">{taller ? 'ORDEN DE TRABAJO' : 'ORDEN DE REPARACIÓN'}</div>
      <div className="recibo-centro recibo-comprobante-numero">N° {r.numero}</div>
      <div className="recibo-linea"></div>
      <Fila etiqueta="Recibido" valor={fechaCorta(r.fecha_creacion)} />
      <Fila etiqueta="Para el" valor={`${fechaCorta(r.fecha_entrega)}${r.hora_entrega ? ` ${r.hora_entrega}` : ''}`} />
      <Fila etiqueta="Cliente" valor={r.cliente_nombre} />
      <Fila etiqueta="Celular" valor={r.cliente_telefono} />
      <div className="recibo-linea"></div>
      <Fila etiqueta="Equipo" valor={`${TIPO[r.tipo_equipo]?.label || r.tipo_equipo}${r.marca || r.modelo ? ` ${[r.marca, r.modelo].filter(Boolean).join(' ')}` : ''}`} />
      <Fila etiqueta="Color" valor={r.color} />
      <Fila etiqueta={r.tipo_equipo === 'CELULAR' ? 'IMEI/Serie' : 'Serie'} valor={r.serie} />
      {taller && <Fila etiqueta="Clave" valor={r.clave} />}
      <Fila etiqueta="Técnico" valor={taller ? r.tecnico : null} />
      {r.accesorios && (
        <>
          <div className="recibo-seccion-titulo">DEJA</div>
          <div className="enc-ticket-detalle">{r.accesorios}</div>
        </>
      )}
      {r.estado_fisico && (
        <>
          <div className="recibo-seccion-titulo">CÓMO LLEGA</div>
          <div className="enc-ticket-detalle">{r.estado_fisico}</div>
        </>
      )}
      <div className="recibo-seccion-titulo">FALLA</div>
      <div className="enc-ticket-detalle">{r.falla}</div>
      {r.diagnostico && (
        <>
          <div className="recibo-seccion-titulo">DIAGNÓSTICO</div>
          <div className="enc-ticket-detalle">{r.diagnostico}</div>
        </>
      )}
      {r.items.length > 0 && (
        <>
          <div className="recibo-linea"></div>
          {r.items.map((i) => (
            <div key={i.id} className="recibo-item">
              <div className="recibo-item-nombre">
                {taller ? `${formatoCantidad(i.cantidad)} × ` : ''}
                {i.nombre}
              </div>
              {i.detalle && <div className="enc-ticket-detalle">{i.detalle}</div>}
              {!taller && (
                <div className="recibo-item-detalle">
                  <span>
                    {formatoCantidad(i.cantidad)} x S/.{i.precio_unitario.toFixed(2)}
                  </span>
                  <span>S/.{i.total.toFixed(2)}</span>
                </div>
              )}
            </div>
          ))}
        </>
      )}
      {r.notas && (
        <>
          <div className="recibo-seccion-titulo">NOTAS</div>
          <div className="enc-ticket-detalle">{r.notas}</div>
        </>
      )}
      {!taller && (
        <>
          <div className="recibo-linea-doble"></div>
          {r.total > 0 ? (
            <>
              <Fila etiqueta="Presupuesto" valor={`S/.${r.total.toFixed(2)}`} />
              <Fila etiqueta="Adelanto" valor={`S/.${pagado.toFixed(2)}`} />
              {r.estado === 'ENTREGADO' ? (
                <div className="recibo-total">
                  <span>{r.a_cuenta ? 'DEBE (A CUENTA)' : 'PAGADO'}</span>
                  <span>{r.a_cuenta ? `S/.${r.saldo.toFixed(2)}` : `S/.${r.total.toFixed(2)}`}</span>
                </div>
              ) : (
                <div className="recibo-total">
                  <span>SALDO</span>
                  <span>S/.{r.saldo.toFixed(2)}</span>
                </div>
              )}
            </>
          ) : (
            <div className="recibo-centro recibo-dato-tienda">El presupuesto se le avisa después del diagnóstico.</div>
          )}
          {r.garantia_dias > 0 && (
            <div className="recibo-centro recibo-dato-tienda">
              Garantía: {r.garantia_dias} días desde la entrega{r.garantia_hasta ? ` (hasta ${fechaCorta(r.garantia_hasta)})` : ''}.
            </div>
          )}
          <div className="recibo-linea"></div>
          <div className="recibo-centro recibo-disclaimer">
            Presente este ticket para recoger su equipo. No es un comprobante de pago: la boleta se entrega al recoger.
          </div>
        </>
      )}
    </div>
  );
}

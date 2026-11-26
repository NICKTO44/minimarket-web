import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { ChevronRight, Printer, Scale, Search, ShoppingCart, Trash2, X } from 'lucide-react';
import { api } from '../../api/api';
import { abreviaturaUnidad } from '../../utils/unidades';
import TicketCompra from '../../components/TicketCompra';
import { imprimirTicket } from '../../utils/imprimirTicket';
import './CompraPeso.css';

// Con qué se paga (las mismas formas que Ingreso rápido en Compras).
const PAGOS = [
  { valor: 'EFECTIVO_CAJA', label: 'De la caja' },
  { valor: 'EFECTIVO', label: 'Efectivo (no de la caja)' },
  { valor: 'TRANSFERENCIA', label: 'Yape / transferencia' },
];

function soles(n) {
  return Number(n || 0).toFixed(2);
}

// "2,35" o "2.35" -> 2.35 (en el celular muchos escriben con coma).
function numero(texto) {
  const n = parseFloat(String(texto ?? '').replace(',', '.'));
  return Number.isFinite(n) ? n : NaN;
}

/**
 * Módulo "Compra por peso" (recicladoras, chatarrerías): se toca el material,
 * se pone el peso y lo que se paga por kilo (viene el último precio de
 * compra; se cambia si hoy subió o bajó). Al pagar: sale de la caja, sube el
 * stock de cada material y se imprime el ticket para quien trajo el material.
 */
export default function CompraPeso() {
  const [productos, setProductos] = useState([]);
  const [tienda, setTienda] = useState(null);
  const [buscar, setBuscar] = useState('');
  const [categoria, setCategoria] = useState('TODAS');
  const [lineas, setLineas] = useState([]);
  const [elegido, setElegido] = useState(null); // { producto, cantidad, precio }
  const [persona, setPersona] = useState('');
  const [pago, setPago] = useState('EFECTIVO_CAJA');
  const [pagando, setPagando] = useState(false);
  const [error, setError] = useState('');
  const [aviso, setAviso] = useState('');
  const [ticket, setTicket] = useState(null);
  // Celular: la compra se abre a pantalla completa desde la barra de abajo (como el carrito del punto de venta).
  const [compraAbierta, setCompraAbierta] = useState(false);
  const numeroTicket = useRef(0);
  const pesoRef = useRef(null);

  const cargarProductos = () =>
    api
      .productos()
      .then((p) => setProductos(p.filter((x) => x.activo)))
      .catch((e) => setError(e.message));

  useEffect(() => {
    cargarProductos();
    api
      .configuracionObtener()
      .then(setTienda)
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!ticket) return undefined;
    const t = setTimeout(() => imprimirTicket(), 80);
    return () => clearTimeout(t);
  }, [ticket]);

  // El cursor va al peso solo al abrir la ventanita de un material (no en
  // cada tecla: si no, al borrar el precio saltaba al peso).
  const idElegido = elegido?.producto.id;
  useEffect(() => {
    if (idElegido) setTimeout(() => pesoRef.current?.focus(), 50);
  }, [idElegido]);

  const categorias = useMemo(
    () => [...new Set(productos.map((p) => p.categoria_nombre).filter(Boolean))].sort(),
    [productos]
  );
  const visibles = useMemo(() => {
    const t = buscar.trim().toLowerCase();
    return productos.filter(
      (p) =>
        (categoria === 'TODAS' || p.categoria_nombre === categoria) &&
        (!t || p.nombre.toLowerCase().includes(t) || (p.codigo || '').toLowerCase().includes(t))
    );
  }, [productos, buscar, categoria]);

  const total = lineas.reduce((s, l) => s + l.cantidad * l.precio, 0);

  const abrir = (producto) => {
    const ya = lineas.find((l) => l.producto.id === producto.id);
    setElegido({
      producto,
      cantidad: ya ? String(ya.cantidad) : '',
      precio: String(ya ? ya.precio : producto.precio_compra || ''),
    });
  };

  const cantidadElegida = elegido ? numero(elegido.cantidad) : NaN;
  const precioElegido = elegido ? numero(elegido.precio) : NaN;
  const elegidoValido = cantidadElegida > 0 && precioElegido >= 0;

  const agregar = (e) => {
    e?.preventDefault();
    if (!elegidoValido) return;
    setLineas((ls) => {
      const sin = ls.filter((l) => l.producto.id !== elegido.producto.id);
      return [...sin, { producto: elegido.producto, cantidad: cantidadElegida, precio: precioElegido }];
    });
    setElegido(null);
    setError('');
    setAviso('');
  };

  const pagar = async () => {
    if (!lineas.length || pagando) return;
    setPagando(true);
    setError('');
    try {
      const hecha = await api.compraPeso({
        persona: persona.trim() || null,
        tipo_pago: pago,
        lineas: lineas.map((l) => ({ producto_id: l.producto.id, cantidad: l.cantidad, precio: l.precio })),
      });
      setTicket({ compra: hecha, clave: ++numeroTicket.current });
      setAviso(
        `Compra ${hecha.folio} registrada: S/ ${soles(hecha.total)} ${
          pago === 'EFECTIVO_CAJA' ? 'pagados de la caja' : `pagados con ${PAGOS.find((p) => p.valor === pago)?.label.toLowerCase()}`
        }.`
      );
      setLineas([]);
      setPersona('');
      cargarProductos(); // el último precio de compra de cada material
    } catch (e) {
      setError(e.message);
    } finally {
      setPagando(false);
    }
  };

  return (
    <div className="cpe-layout">
      <div className="cpe-top">
        <h1>
          <Scale size={22} /> Compra por peso
        </h1>
      </div>

      <div className="cpe-cuerpo">
        <section className="cpe-materiales">
          <label className="cpe-buscar">
            <Search size={16} />
            <input value={buscar} onChange={(e) => setBuscar(e.target.value)} placeholder="Buscar material" />
          </label>
          {categorias.length > 1 && (
            <div className="cpe-categorias">
              {['TODAS', ...categorias].map((c) => (
                <button key={c} type="button" className={categoria === c ? 'activo' : ''} onClick={() => setCategoria(c)}>
                  {c === 'TODAS' ? 'Todos' : c}
                </button>
              ))}
            </div>
          )}
          <div className="cpe-grilla">
            {visibles.map((p) => {
              const enCompra = lineas.some((l) => l.producto.id === p.id);
              return (
                <button key={p.id} type="button" className={`cpe-material${enCompra ? ' en-compra' : ''}`} onClick={() => abrir(p)}>
                  <span className="cpe-material-nombre">{p.nombre}</span>
                  <span className="cpe-material-precio">
                    S/ {soles(p.precio_compra)} / {abreviaturaUnidad(p.unidad_medida)}
                  </span>
                </button>
              );
            })}
            {visibles.length === 0 && <p className="cpe-vacio">No hay materiales. Créalos en Productos con su precio de compra.</p>}
          </div>
        </section>

        <aside className={`cpe-compra${compraAbierta ? ' cpe-compra-abierta' : ''}`}>
          <div className="cpe-compra-cabecera">
            <h3>Compra actual</h3>
            <button type="button" className="cpe-compra-cerrar" onClick={() => setCompraAbierta(false)} aria-label="Volver a los materiales">
              <X size={18} />
            </button>
          </div>
          {lineas.length === 0 ? (
            <p className="cpe-vacio">Toca un material para ponerle el peso.</p>
          ) : (
            <div className="cpe-lineas">
              {lineas.map((l) => (
                <div key={l.producto.id} className="cpe-linea">
                  <button type="button" className="cpe-linea-texto" onClick={() => abrir(l.producto)}>
                    <strong>{l.producto.nombre}</strong>
                    <span>
                      {l.cantidad} {abreviaturaUnidad(l.producto.unidad_medida)} × S/ {soles(l.precio)}
                    </span>
                  </button>
                  <span className="cpe-linea-total">S/ {soles(l.cantidad * l.precio)}</span>
                  <button
                    type="button"
                    className="cpe-quitar"
                    onClick={() => setLineas((ls) => ls.filter((x) => x.producto.id !== l.producto.id))}
                    aria-label={`Quitar ${l.producto.nombre}`}
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              ))}
            </div>
          )}
          <label className="cpe-persona">
            <span>Quien trae el material (opcional)</span>
            <input value={persona} onChange={(e) => setPersona(e.target.value)} placeholder="Nombre" maxLength={80} />
          </label>
          <div className="cpe-pagos" role="radiogroup" aria-label="Con qué se paga">
            {PAGOS.map((p) => (
              <button
                key={p.valor}
                type="button"
                role="radio"
                aria-checked={pago === p.valor}
                className={pago === p.valor ? 'activo' : ''}
                onClick={() => setPago(p.valor)}
              >
                {p.label}
              </button>
            ))}
          </div>
          <div className="cpe-total">
            <span>Total a pagar</span>
            <strong>S/ {soles(total)}</strong>
          </div>
          {error && <p className="cpe-error">{error}</p>}
          {aviso && <p className="cpe-aviso">{aviso}</p>}
          <button type="button" className="cpe-pagar" onClick={pagar} disabled={!lineas.length || pagando}>
            <Printer size={18} /> {pagando ? 'Registrando...' : `Pagar S/ ${soles(total)} e imprimir`}
          </button>
          {ticket && (
            <button type="button" className="cpe-reimprimir" onClick={() => setTicket({ ...ticket, clave: ++numeroTicket.current })}>
              Reimprimir el último ticket
            </button>
          )}
        </aside>
      </div>

      {/* Solo en celular: barra de la compra abajo; al tocarla se abre para pagar. */}
      {lineas.length > 0 && !compraAbierta && (
        <button type="button" className="cpe-barra" onClick={() => setCompraAbierta(true)} aria-label="Abrir la compra para pagar">
          <ShoppingCart size={18} />
          <span className="cpe-barra-numero">{lineas.length}</span>
          <span>{lineas.length === 1 ? 'material' : 'materiales'}</span>
          <span className="cpe-barra-total">
            S/ {soles(total)} · Pagar <ChevronRight size={16} />
          </span>
        </button>
      )}

      {elegido && (
        <div className="cpe-modal-fondo" onClick={() => setElegido(null)}>
          <form className="cpe-modal" onClick={(e) => e.stopPropagation()} onSubmit={agregar}>
            <div className="cpe-modal-cabecera">
              <h3>{elegido.producto.nombre}</h3>
              <button type="button" onClick={() => setElegido(null)} aria-label="Cerrar">
                <X size={18} />
              </button>
            </div>
            <label>
              <span>Peso ({abreviaturaUnidad(elegido.producto.unidad_medida)})</span>
              <input
                ref={pesoRef}
                inputMode="decimal"
                value={elegido.cantidad}
                onChange={(e) => setElegido({ ...elegido, cantidad: e.target.value })}
                placeholder="0.00"
              />
            </label>
            <label>
              <span>Precio por {abreviaturaUnidad(elegido.producto.unidad_medida)} (S/)</span>
              <input inputMode="decimal" value={elegido.precio} onChange={(e) => setElegido({ ...elegido, precio: e.target.value })} />
            </label>
            <div className="cpe-modal-total">
              Subtotal: <strong>S/ {soles(elegidoValido ? cantidadElegida * precioElegido : 0)}</strong>
            </div>
            <button type="submit" className="cpe-pagar" disabled={!elegidoValido}>
              Agregar a la compra
            </button>
          </form>
        </div>
      )}

      {ticket && createPortal(<TicketCompra key={ticket.clave} compra={ticket.compra} tienda={tienda} />, document.body)}
    </div>
  );
}

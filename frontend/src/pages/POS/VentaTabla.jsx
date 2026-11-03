import { useEffect, useState } from 'react';
import { formatoPrecio, seVendeFraccionado, subtotalLinea } from '../../utils/medidas';
import { etiquetaUnidad } from '../../utils/unidades';
import { opcionesDePrecio } from '../../utils/tresPrecios';
import './VentaTabla.css';

// Módulo "Venta en tabla" (Lubricentro): en vez de tarjetas, el buscador
// muestra una lista y lo elegido entra como una fila de una tabla, como en
// los sistemas de facturación de escritorio. Solo dibuja: el carrito, el
// stock, los precios permitidos y el cobro siguen siendo los de POS.jsx.

const TASA_IGV = 0.18;
const r2 = (n) => Math.round((Number(n) + Number.EPSILON) * 100) / 100;
const soles = (n) => `S/ ${formatoPrecio(r2(n))}`;

/** Precio sin IGV de una línea (exonerados e inafectos no llevan IGV). */
const sinIgv = (precio, afectacion) => (!afectacion || afectacion === 'GRAVADO' ? precio / (1 + TASA_IGV) : precio);

/** Número escrito ("12,5", "") -> número o null. */
const leer = (texto) => {
  const t = String(texto ?? '').trim().replace(',', '.');
  if (t === '') return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : NaN;
};

// Campo que se edita libre y se aplica al salir o con Enter (Esc vuelve atrás).
function CampoNumero({ valor, onAplicar, etiqueta, className, decimales = 2 }) {
  const mostrar = (v) => (v == null || v === 0 ? '' : decimales === 0 ? String(v) : Number(v).toFixed(decimales));
  const [texto, setTexto] = useState(mostrar(valor));
  // Si el valor cambia desde afuera (− y +, P1/P2/P3), el campo lo muestra.
  const [previo, setPrevio] = useState(valor);
  if (previo !== valor) {
    setPrevio(valor);
    setTexto(mostrar(valor));
  }
  const aplicar = () => {
    const n = leer(texto);
    if (n === null ? valor == null || valor === 0 : Math.abs(n - (valor || 0)) < 0.0005) {
      setTexto(mostrar(valor));
      return;
    }
    if (!onAplicar(n)) setTexto(mostrar(valor));
  };
  return (
    <input
      type="text"
      inputMode="decimal"
      className={className}
      aria-label={etiqueta}
      value={texto}
      placeholder={decimales === 0 ? '' : '0.00'}
      onChange={(e) => setTexto(e.target.value)}
      onFocus={(e) => e.target.select()}
      onBlur={aplicar}
      onKeyDown={(e) => {
        if (e.key === 'Enter') e.currentTarget.blur();
        if (e.key === 'Escape') {
          setTexto(mostrar(valor));
          e.currentTarget.blur();
        }
      }}
    />
  );
}

/** Lista que aparece bajo el buscador mientras se escribe. */
export function ResultadosTabla({ resultados, resaltado, onElegir, onResaltar, disponibleDe, preciosExtra, marcaDe, total }) {
  if (!resultados) return null;
  return (
    <div className="vt-resultados" role="listbox" aria-label="Productos encontrados">
      {resultados.length === 0 ? (
        <p className="vt-resultados-vacio">No se encontró ningún producto.</p>
      ) : (
        <>
          <p className="vt-resultados-ayuda">
            {total > resultados.length ? `${resultados.length} de ${total} resultados` : `${total} resultado${total === 1 ? '' : 's'}`} · ↑ ↓ para
            elegir · Enter para agregar
          </p>
          {resultados.map((p, i) => {
            const stock = p.controla_stock === false ? null : disponibleDe(p.id);
            const opciones = opcionesDePrecio(p.precio, preciosExtra?.get(p.id));
            const marca = marcaDe(p);
            return (
              <button
                key={p.id}
                type="button"
                role="option"
                aria-selected={i === resaltado}
                className={`vt-resultado${i === resaltado ? ' vt-resultado-activo' : ''}`}
                onMouseEnter={() => onResaltar(i)}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => onElegir(p)}
              >
                <span className="vt-resultado-info">
                  <span className="vt-resultado-nombre">{p.nombre}</span>
                  <span className="vt-resultado-detalle">
                    {[p.codigo, marca, p.descripcion].filter(Boolean).join(' · ')}
                  </span>
                </span>
                <span className="vt-resultado-datos">
                  {stock != null && (
                    <span className={stock > 0 ? 'vt-stock' : 'vt-stock vt-stock-sin'}>{stock > 0 ? `Stock ${Number(r2(stock))}` : 'Sin stock'}</span>
                  )}
                  {opciones.length > 1 ? (
                    opciones.map((o) => (
                      <span key={o.numero} className="vt-precio-chip">
                        P{o.numero} {formatoPrecio(o.precio)}
                      </span>
                    ))
                  ) : (
                    <span className="vt-precio-chip">{soles(p.precio)}</span>
                  )}
                </span>
              </button>
            );
          })}
        </>
      )}
    </div>
  );
}

/** La venta como tabla, con subtotal, descuento y total abajo. */
export default function VentaTabla({
  carrito,
  claveDe,
  marcaDe,
  preciosExtra,
  tresPrecios,
  precioEditable,
  onCantidad,
  onSumar,
  onPrecio,
  onDescuento,
  onQuitar,
  esFija = () => false,
  // El producto del catálogo: completa lo que no trae una línea cargada de una
  // orden de servicio (código, aplicación, categoría, costo, IGV).
  productoDe = () => null,
  onCobrar,
  cobroAbierto = false,
  onCerrarCobro,
}) {
  const subtotal = r2(carrito.reduce((s, i) => s + subtotalLinea(i.precio, i.cantidad), 0));
  const descuento = r2(carrito.reduce((s, i) => s + (i.descuentoMonto || 0), 0));
  const hayVenta = carrito.length > 0;

  // F9 abre el cobro (Esc lo cierra, si está abierto).
  useEffect(() => {
    const tecla = (e) => {
      if (e.key === 'F9' && hayVenta && !cobroAbierto) {
        e.preventDefault();
        onCobrar();
      } else if (e.key === 'Escape' && cobroAbierto) {
        onCerrarCobro();
      }
    };
    window.addEventListener('keydown', tecla);
    return () => window.removeEventListener('keydown', tecla);
  }, [hayVenta, cobroAbierto, onCobrar, onCerrarCobro]);

  return (
    <div className="vt">
      <div className="vt-tabla-caja">
        <table className="vt-tabla">
          <thead>
            <tr>
              <th scope="col" className="vt-col-n">#</th>
              <th scope="col">Productos o Servicios</th>
              <th scope="col">Marca</th>
              <th scope="col">Categoría</th>
              <th scope="col">Unidad</th>
              <th scope="col">Cantidad</th>
              <th scope="col" className="vt-num">Valor U.</th>
              <th scope="col" className="vt-num">P. sin IGV</th>
              <th scope="col">Precio U.</th>
              <th scope="col">Desc. S/</th>
              <th scope="col" className="vt-num">Importe</th>
              <th scope="col" className="vt-col-quitar">
                <span className="vt-oculto">Quitar</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {carrito.length === 0 && (
              <tr>
                <td colSpan={12} className="vt-vacio">
                  Busca un producto arriba (nombre, código, aplicación o N.° de parte) y presiona Enter para agregarlo.
                </td>
              </tr>
            )}
            {carrito.map((item, idx) => {
              const clave = claveDe(item);
              const deLinea = subtotalLinea(item.precio, item.cantidad);
              const importe = r2(deLinea - (item.descuentoMonto || 0));
              const opciones =
                tresPrecios && !item.clave && !item.presentacion
                  ? opcionesDePrecio(item.precioNormal ?? item.precioOriginal ?? item.precio, preciosExtra?.get(item.id))
                  : [];
              const fraccion = seVendeFraccionado(item.unidad_medida);
              const fija = esFija(item);
              const catalogo = productoDe(item.id);
              const dato = (campo) => item[campo] ?? catalogo?.[campo];
              // P. sin IGV de lo que realmente se cobra: con descuento, baja igual que en la factura.
              const precioReal = item.descuentoMonto > 0 && item.cantidad > 0 ? importe / item.cantidad : item.precio;
              return (
                <tr key={clave}>
                  <td className="vt-col-n">{idx + 1}</td>
                  <td className="vt-producto">
                    <span className="vt-nombre">{item.nombre}</span>
                    {item.detalle && <span className="vt-sub">{item.detalle}</span>}
                    <span className="vt-sub">{dato('codigo')}</span>
                    {dato('descripcion') && <span className="vt-sub vt-aplicacion">{dato('descripcion')}</span>}
                    {item.paquete && <span className="vt-sub">Paquete {item.paquete.nombre}</span>}
                  </td>
                  <td>{marcaDe(item) || '—'}</td>
                  <td>{dato('categoria_nombre') || '—'}</td>
                  <td>{item.presentacion ? item.presentacion.nombre || etiquetaUnidad(item.unidad_medida) : etiquetaUnidad(item.unidad_medida)}</td>
                  <td>
                    {fija ? (
                      <span className="vt-cantidad-fija">{item.presentacion ? item.cantidadPresentacion : item.cantidad}</span>
                    ) : (
                      <div className="vt-cantidad">
                        <button type="button" aria-label={`Quitar uno de ${item.nombre}`} onClick={() => onSumar(clave, -1)}>
                          −
                        </button>
                        {item.presentacion ? (
                          <span className="vt-cantidad-fija">{item.cantidadPresentacion}</span>
                        ) : (
                          <CampoNumero
                            className="vt-input vt-input-cantidad"
                            etiqueta={`Cantidad de ${item.nombre}`}
                            valor={item.cantidad}
                            decimales={fraccion ? 3 : 0}
                            onAplicar={(n) => {
                              if (n == null || Number.isNaN(n) || n <= 0) return false;
                              if (!fraccion && !Number.isInteger(n)) return false;
                              onCantidad(clave, n);
                              return true;
                            }}
                          />
                        )}
                        <button type="button" aria-label={`Agregar uno de ${item.nombre}`} onClick={() => onSumar(clave, 1)}>
                          +
                        </button>
                      </div>
                    )}
                  </td>
                  <td className="vt-num">{dato('precio_compra') > 0 ? soles(dato('precio_compra')) : '—'}</td>
                  <td className="vt-num">{soles(sinIgv(precioReal, dato('afectacion_igv')))}</td>
                  <td>
                    {precioEditable && !item.presentacion && !fija ? (
                      <CampoNumero
                        className="vt-input vt-input-precio"
                        etiqueta={`Precio de ${item.nombre}`}
                        valor={item.precio}
                        onAplicar={(n) => {
                          if (n == null || Number.isNaN(n) || n <= 0) return false;
                          onPrecio(clave, r2(n));
                          return true;
                        }}
                      />
                    ) : (
                      <span className="vt-precio-fijo">{formatoPrecio(item.precio)}</span>
                    )}
                    {opciones.length > 1 && (
                      <div className="vt-listas" role="group" aria-label={`Lista de precios de ${item.nombre}`}>
                        {opciones.map((o) => {
                          const elegido = Math.abs(Number(item.precio) - Number(o.precio)) < 0.005;
                          return (
                            <button
                              key={o.numero}
                              type="button"
                              className={`vt-lista${elegido ? ' vt-lista-activa' : ''}`}
                              aria-pressed={elegido}
                              title={`${['Precio público', 'Precio tienda', 'Precio por mayor'][o.numero - 1]}: S/ ${formatoPrecio(o.precio)}`}
                              onClick={() => !elegido && onPrecio(clave, Number(o.precio))}
                            >
                              P{o.numero}
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </td>
                  <td>
                    {item.paquete ? (
                      <span className="vt-precio-fijo">{formatoPrecio(item.descuentoMonto || 0)}</span>
                    ) : (
                      <CampoNumero
                        className="vt-input vt-input-descuento"
                        etiqueta={`Descuento de ${item.nombre}`}
                        valor={item.descuentoMonto || 0}
                        onAplicar={(n) => {
                          const monto = n == null ? 0 : n;
                          if (Number.isNaN(monto) || monto < 0 || monto > deLinea + 0.0001) return false;
                          onDescuento(clave, r2(monto));
                          return true;
                        }}
                      />
                    )}
                  </td>
                  <td className="vt-num vt-importe">{soles(importe)}</td>
                  <td className="vt-col-quitar">
                    <button
                      type="button"
                      className="vt-quitar"
                      aria-label={item.paquete ? `Quitar el paquete ${item.paquete.nombre}` : `Quitar ${item.nombre}`}
                      title={item.paquete ? 'Quitar el paquete completo' : 'Quitar'}
                      onClick={() => onQuitar(item)}
                    >
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                        <path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3" />
                      </svg>
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {hayVenta && (
        <table className="vt-totales">
          <tbody>
            <tr>
              <th scope="row">Subtotal</th>
              <td>{soles(subtotal)}</td>
            </tr>
            <tr>
              <th scope="row">Descuento</th>
              <td>{descuento > 0 ? `− ${soles(descuento)}` : soles(0)}</td>
            </tr>
            <tr className="vt-total">
              <th scope="row">Total</th>
              <td>{soles(subtotal - descuento)}</td>
            </tr>
          </tbody>
        </table>
      )}
      <div className="vt-barra">
        <span className="vt-barra-info">
          {hayVenta
            ? `${carrito.length} producto${carrito.length === 1 ? '' : 's'}`
            : 'Agrega productos para cobrar'}
        </span>
        <button type="button" className="vt-cobrar" disabled={!hayVenta} onClick={onCobrar}>
          Cobrar {soles(subtotal - descuento)} <span className="vt-tecla">F9</span>
        </button>
      </div>
      {cobroAbierto && <div className="vt-fondo" onClick={onCerrarCobro} aria-hidden="true" />}
    </div>
  );
}

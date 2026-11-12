import { useRef, useState } from 'react';
import { api } from '../../api/api';
import { ErrorDeArchivo, TIPO_XLSX, escribirLibro, leerHoja } from '../../utils/hojas';
import { descargarArchivo } from '../../utils/pdfTicket';
import { hoyLima } from '../../utils/formato';
import { mapaPorProducto } from '../../utils/repuestos';
import { mapaPrecios } from '../../utils/tresPrecios';
import { FILTROS_PEDIDO, hojaPedido, leerPedido, productosDelPedido } from '../../utils/pedidoProveedor';
import LeerExcelProveedor from './LeerExcelProveedor';
import { etiquetaUnidad, opcionesUnidad } from '../../utils/unidades';
import './PedidoExcel.css';

// Módulos "Venta en tabla" y "Compras por Excel": pedido a proveedor por Excel (ver utils/pedidoProveedor.js).

// Ficha del repuesto (marca, n.° de parte) y Precio 2 / 3: si el negocio no
// tiene esos módulos, las columnas salen vacías y el pedido funciona igual.
async function datosExtra(repuestos = true) {
  const [fichas, precios] = await Promise.all([
    repuestos ? api.repuestosFichas().catch(() => []) : [],
    api.preciosVenta().catch(() => []),
  ]);
  return { fichas: mapaPorProducto(fichas), precios: mapaPrecios(precios) };
}

/** Ventana "Exportar pedido". */
export function ExportarPedido({ productos, categorias, repuestos = true, onCerrar }) {
  const [filtro, setFiltro] = useState('BAJO_MINIMO');
  const [categoriaId, setCategoriaId] = useState('');
  // Con el stock del negocio (para él); sin stock, para mandárselo al proveedor.
  const [conStock, setConStock] = useState(true);
  const [ocupado, setOcupado] = useState(false);
  const [error, setError] = useState(null);

  const lista = productosDelPedido(productos, { filtro, categoriaId });

  const descargar = async () => {
    if (lista.length === 0 || ocupado) return;
    setOcupado(true);
    setError(null);
    try {
      const { fichas, precios } = await datosExtra(repuestos);
      // La cantidad sugerida solo con "bajo stock mínimo"; con todos o una categoría la columna va vacía.
      const hoja = hojaPedido(lista, { completo: false, fichas, precios, sugerir: filtro === 'BAJO_MINIMO', conStock, repuestos });
      const nombre = `Pedido-${hoyLima()}.xlsx`;
      descargarArchivo(escribirLibro([hoja]), nombre, { tipo: TIPO_XLSX });
      onCerrar();
    } catch (e) {
      setError(`No se pudo armar el Excel: ${e.message}`);
    } finally {
      setOcupado(false);
    }
  };

  return (
    <div className="prov-modal-overlay" onClick={onCerrar}>
      <div className="prov-modal pedido-modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-labelledby="pedido-titulo">
        <h2 id="pedido-titulo">Exportar pedido</h2>
        <p className="pedido-ayuda">
          Llena en este Excel <strong>la cantidad</strong> y <strong>el precio unitario</strong> de lo que compraste (o envíaselo a tu
          proveedor para que lo llene). Abajo se pueden agregar productos nuevos. Luego súbelo en{' '}
          <strong>Nueva compra → Cargar desde Excel</strong> y entran todos de un golpe. No lleva tus precios de venta.
        </p>

        <fieldset className="pedido-grupo">
          <legend>¿Qué productos?</legend>
          {FILTROS_PEDIDO.map((f) => (
            <label key={f.valor} className="pedido-opcion">
              <input type="radio" name="pedido-filtro" value={f.valor} checked={filtro === f.valor} onChange={() => setFiltro(f.valor)} />
              {f.label}
            </label>
          ))}
          {filtro === 'CATEGORIA' && (
            <select className="pedido-select" value={categoriaId} onChange={(e) => setCategoriaId(e.target.value)} aria-label="Categoría">
              <option value="">Elige la categoría…</option>
              {categorias.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.nombre}
                </option>
              ))}
            </select>
          )}
        </fieldset>

        <label className="pedido-opcion pedido-con-stock">
          <input type="checkbox" checked={conStock} onChange={(e) => setConStock(e.target.checked)} />
          <span>
            <strong>Exportar con mi stock</strong> (columna "Stock actual", para saber cuánto pedir)
          </span>
        </label>
        {conStock ? (
          <p className="pedido-aviso-stock">
            Si este Excel es para enviárselo al proveedor, desmarca la casilla: se exportará sin tu stock.
          </p>
        ) : (
          <p className="pedido-aviso-stock pedido-aviso-stock-ok">Se exportará sin tu stock: listo para enviárselo al proveedor.</p>
        )}

        <p className="pedido-cuenta">
          {lista.length === 0
            ? filtro === 'BAJO_MINIMO'
              ? 'Ningún producto está en su stock mínimo o debajo.'
              : filtro === 'CATEGORIA' && !categoriaId
                ? 'Elige una categoría.'
                : 'No hay productos para exportar.'
            : `${lista.length} producto${lista.length === 1 ? '' : 's'}${filtro === 'BAJO_MINIMO' ? ', con la cantidad sugerida para llegar al mínimo' : ''}.`}
        </p>
        {error && <p className="pedido-error">{error}</p>}

        <div className="prov-modal-acciones">
          <button type="button" className="prov-boton-cancelar" onClick={onCerrar}>
            Cancelar
          </button>
          <button type="button" className="prov-boton-guardar" disabled={lista.length === 0 || ocupado} onClick={descargar}>
            {ocupado ? 'Armando…' : '⬇ Descargar Excel'}
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * Botón "Cargar desde Excel" dentro de Nueva compra. onCargar(items, precios)
 * recibe los productos para la compra y los precios de venta que cambian;
 * debajo del botón queda el resumen de lo leído. El Excel del sistema
 * ("Exportar pedido") entra directo; el de un proveedor (cualquier formato)
 * pasa por LeerExcelProveedor: columnas, productos y lo que se aprende.
 */
export function CargarPedido({ productos, repuestos = true, proveedorId = null, presentaciones = null, onCargar }) {
  const entrada = useRef(null);
  const [ocupado, setOcupado] = useState(false);
  const [aviso, onAviso] = useState(null);
  // Excel de un proveedor en revisión: { filas, fichas, precios }.
  const [enRevision, setEnRevision] = useState(null);

  // Arma la compra con una hoja "como la del sistema" y muestra el resumen.
  const aplicar = (filas, fichas, precios, extra = '') => {
    const r = leerPedido(filas, productos, fichas, precios, presentaciones);
    if (r.error) {
      onAviso({ tipo: 'error', texto: r.error });
      return false;
    }
    if (r.items.length === 0 && r.nuevos.length === 0 && r.precios.length === 0 && r.sinReconocer.length === 0 && r.ambiguos.length === 0 && r.malas.length === 0) {
      onAviso({ tipo: 'error', texto: 'El Excel no tiene ninguna fila con cantidad.' });
      return false;
    }
    onCargar(r.items, r.precios, r.subidas, r.nuevos, precios.size > 0);
    const partes = [`Se cargaron ${r.items.length} producto${r.items.length === 1 ? '' : 's'}. Revisa cantidades y costos antes de guardar.`];
    if (extra) partes.push(extra);
    if (r.convertidos > 0) {
      partes.push(`${r.convertidos} llegaron por caja o blíster: se pasaron a unidades (míralo debajo de cada producto).`);
    }
    if (r.subidas.length > 0) partes.push(`${r.subidas.length} subieron de costo: revisa sus precios de venta (recuadro amarillo).`);
    if (r.nuevos.length > 0) partes.push(`${r.nuevos.length} producto${r.nuevos.length === 1 ? ' nuevo' : 's nuevos'}: completa sus datos (recuadro verde).`);
    if (r.precios.length > 0) {
      partes.push(`${r.precios.length} producto${r.precios.length === 1 ? '' : 's'} con precio de venta nuevo (ver abajo).`);
    }
    if (r.sinCosto > 0) partes.push(`${r.sinCosto} sin costo: escríbelo en la lista.`);
    const lista = (xs) => xs.slice(0, 6).map((x) => `fila ${x.fila} (${x.texto})`).join(', ') + (xs.length > 6 ? `… y ${xs.length - 6} más` : '');
    if (r.sinReconocer.length) partes.push(`No se reconocieron: ${lista(r.sinReconocer)}.`);
    if (r.ambiguos.length) partes.push(`N.° de parte en más de un producto (agrégalos a mano): ${lista(r.ambiguos)}.`);
    if (r.malas.length) partes.push(`Con error: ${lista(r.malas)}.`);
    onAviso({ tipo: r.sinReconocer.length || r.ambiguos.length || r.malas.length || r.sinCosto ? 'error' : 'exito', texto: partes.join(' ') });
    return true;
  };

  const cargar = async (archivo) => {
    if (!archivo) return;
    setOcupado(true);
    onAviso(null);
    setEnRevision(null);
    try {
      const [filas, { fichas, precios }] = await Promise.all([leerHoja(archivo), datosExtra(repuestos)]);
      // El Excel del sistema (o uno que se reconoce entero) entra directo.
      const directo = leerPedido(filas, productos, fichas, precios, presentaciones);
      if (!directo.error && directo.sinReconocer.length === 0 && directo.ambiguos.length === 0) {
        aplicar(filas, fichas, precios);
        return;
      }
      if (!proveedorId) {
        onAviso({
          tipo: 'error',
          texto: 'Este Excel no es el del sistema. Elige primero el proveedor de la compra: así el sistema lee su Excel y lo recuerda para la próxima vez.',
        });
        return;
      }
      setEnRevision({ filas, fichas, precios });
    } catch (e) {
      onAviso({
        tipo: 'error',
        texto: e instanceof ErrorDeArchivo ? e.message : 'No se pudo leer el archivo. Revisa que sea un Excel (.xlsx) o un CSV.',
      });
    } finally {
      setOcupado(false);
      if (entrada.current) entrada.current.value = '';
    }
  };

  return (
    <div className="pedido-carga">
      <button type="button" className="pedido-cargar" disabled={ocupado} onClick={() => entrada.current?.click()}>
        {ocupado ? 'Leyendo…' : '📄 Cargar desde Excel'}
      </button>
      <input
        ref={entrada}
        type="file"
        accept=".xlsx,.csv,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,text/csv"
        hidden
        onChange={(e) => cargar(e.target.files?.[0])}
      />
      {enRevision && (
        <LeerExcelProveedor
          filas={enRevision.filas}
          productos={productos}
          fichas={enRevision.fichas}
          proveedorId={proveedorId}
          onCancelar={() => setEnRevision(null)}
          onListo={({ hoja, enlaces, columnas, aprendidos }) => {
            const { fichas, precios } = enRevision;
            if (aplicar(hoja, fichas, precios, aprendidos > 0 ? `El sistema recordará ${aprendidos} producto${aprendidos === 1 ? '' : 's'} de este proveedor.` : '')) {
              setEnRevision(null);
              api.compraExcelAprender(proveedorId, { columnas, enlaces }).catch(() => {});
            }
          }}
        />
      )}
      {aviso && (
        <p className={`pedido-aviso pedido-aviso-${aviso.tipo}`} role="status">
          {aviso.texto}
        </p>
      )}
    </div>
  );
}

const soles = (n) => (n == null ? '—' : Number(n).toFixed(2));
// Costo por unidad: con 4 decimales si es menor a S/ 1 (una tableta a S/ 0.095).
const costoTexto = (n) => (n == null ? '—' : Number(n) < 1 ? Number(n).toFixed(4) : Number(n).toFixed(2));
const plural = (t) => (/[aeiouáéíóú]$/.test(t) ? `${t}s` : /s$/.test(t) ? t : `${t}es`);

/** Precio de antes -> el nuevo (en negrita si cambia). */
function Cambio({ antes, despues }) {
  if (Math.abs(Number(antes || 0) - Number(despues || 0)) < 0.005) return <span className="pedido-igual">{soles(antes)}</span>;
  return (
    <span>
      {soles(antes)} → <strong>{soles(despues)}</strong>
    </span>
  );
}

/**
 * Lista de los precios de venta que cambian con el Excel, con la casilla para
 * aplicarlos al registrar la compra.
 */
export function PreciosNuevos({ cambios, aplicar, onAplicar }) {
  if (!cambios?.length) return null;
  // P2 y P3 solo si alguno de estos productos los tiene.
  const tresPrecios = cambios.some((c) => c.antes.p2 || c.despues.p2 || c.antes.p3 || c.despues.p3);
  return (
    <div className="pedido-precios">
      <label className="pedido-precios-titulo">
        <input type="checkbox" checked={aplicar} onChange={(e) => onAplicar(e.target.checked)} />
        Actualizar también estos precios de venta al registrar la compra ({cambios.length})
      </label>
      <div className="pedido-precios-caja">
        <table className="pedido-precios-tabla">
          <thead>
            <tr>
              <th scope="col">Código</th>
              <th scope="col">Producto</th>
              <th scope="col">P1</th>
              {tresPrecios && <th scope="col">P2</th>}
              {tresPrecios && <th scope="col">P3</th>}
            </tr>
          </thead>
          <tbody>
            {cambios.map((c) => (
              <tr key={c.producto_id}>
                <td>{c.codigo}</td>
                <td>{c.nombre}</td>
                <td>
                  <Cambio antes={c.antes.p1} despues={c.despues.p1} />
                </td>
                {tresPrecios && (
                  <td>
                    <Cambio antes={c.antes.p2} despues={c.despues.p2} />
                  </td>
                )}
                {tresPrecios && (
                  <td>
                    <Cambio antes={c.antes.p3} despues={c.despues.p3} />
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/** Campo de precio que se aplica al salir (deja escribir decimales). */
function CampoPrecio({ valor, onCambio, etiqueta, requerido }) {
  return (
    <input
      key={`${etiqueta}-${valor ?? ''}`}
      type="number"
      min="0"
      step="any"
      inputMode="decimal"
      className={`pedido-campo${requerido && !(Number(valor) > 0) ? ' pedido-campo-falta' : ''}`}
      aria-label={etiqueta}
      defaultValue={valor ?? ''}
      onBlur={(e) => {
        const n = parseFloat(e.target.value);
        onCambio(Number.isFinite(n) && n > 0 ? Math.round(n * 100) / 100 : null);
      }}
    />
  );
}

/** Debajo de cada precio: el de antes y cuánto se gana con el costo nuevo. */
function Ganancia({ antes, precio, costo }) {
  if (!(Number(precio) > 0)) return null;
  const gana = Number(precio) - Number(costo);
  const pct = Math.round((gana / Number(precio)) * 100);
  return (
    <span className={`pedido-ganancia${gana <= 0 ? ' pedido-ganancia-mala' : ''}`}>
      {antes != null && Math.abs(Number(antes) - Number(precio)) >= 0.005 ? `antes ${soles(antes)} · ` : ''}
      gana S/ {soles(gana)} ({pct} %)
    </span>
  );
}

/**
 * 🟡 Productos que suben de costo con esta compra: se muestran sus 3 precios
 * de venta para que el dueño los suba ahí mismo (se aplican al registrar).
 */
export function SubioCosto({ subidas, tresPrecios, onCambio }) {
  if (!subidas?.length) return null;
  const poner = (id, campo, n) =>
    onCambio(subidas.map((s) => (s.producto_id === id ? { ...s, despues: { ...s.despues, [campo]: n ?? (campo === 'p1' ? s.antes.p1 : null) } } : s)));
  return (
    <div className="pedido-revisar pedido-revisar-subio">
      <p className="pedido-revisar-titulo">🟡 Subieron de costo ({subidas.length}): ¿subes tus precios de venta?</p>
      <p className="pedido-revisar-ayuda">
        Están puestos tus precios de hoy. Debajo ves cuánto ganas con el costo nuevo; cambia los que quieras. Si no tocas nada, se quedan igual.
      </p>
      <div className="pedido-revisar-caja">
        <table className="pedido-revisar-tabla">
          <thead>
            <tr>
              <th scope="col">Producto</th>
              <th scope="col">Costo</th>
              <th scope="col">P1</th>
              {tresPrecios && <th scope="col">P2</th>}
              {tresPrecios && <th scope="col">P3</th>}
            </tr>
          </thead>
          <tbody>
            {subidas.map((s) => (
              <tr key={s.producto_id}>
                <td>
                  <span className="prov-codigo">{s.codigo}</span>
                  {s.nombre}
                </td>
                <td className="pedido-revisar-costo">
                  {costoTexto(s.costoAntes)} → <strong>{costoTexto(s.costoNuevo)}</strong>
                </td>
                <td>
                  <CampoPrecio valor={s.despues.p1} etiqueta={`Precio 1 de ${s.nombre}`} onCambio={(n) => poner(s.producto_id, 'p1', n)} />
                  <Ganancia antes={s.antes.p1} precio={s.despues.p1} costo={s.costoNuevo} />
                </td>
                {tresPrecios && (
                  <td>
                    <CampoPrecio valor={s.despues.p2} etiqueta={`Precio 2 de ${s.nombre}`} onCambio={(n) => poner(s.producto_id, 'p2', n)} />
                  <Ganancia antes={s.antes.p2} precio={s.despues.p2} costo={s.costoNuevo} />
                  </td>
                )}
                {tresPrecios && (
                  <td>
                    <CampoPrecio valor={s.despues.p3} etiqueta={`Precio 3 de ${s.nombre}`} onCambio={(n) => poner(s.producto_id, 'p3', n)} />
                  <Ganancia antes={s.antes.p3} precio={s.despues.p3} costo={s.costoNuevo} />
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/**
 * 🟢 Productos que el negocio aún no tiene: llegan con lo que escribió el
 * proveedor; el dueño elige la categoría y pone sus precios de venta. Se
 * crean al registrar la compra.
 */
export function ProductosNuevos({ nuevos, categorias, tresPrecios, repuestos = true, vendeSuelto = false, unidadesActivas = null, faltas, onCambio }) {
  if (!nuevos?.length) return null;
  const poner = (idx, campo, valor) => onCambio(nuevos.map((n, i) => (i === idx ? { ...n, [campo]: valor } : n)));
  return (
    <div className="pedido-revisar pedido-revisar-nuevos">
      <p className="pedido-revisar-titulo">🟢 Productos nuevos ({nuevos.length}): completa lo marcado en rojo</p>
      <p className="pedido-revisar-ayuda">Se crean en Productos al registrar la compra. El código lo pone el sistema (puedes cambiarlo).</p>
      <div className="pedido-nuevos-lista">
        {nuevos.map((n, idx) => {
          const falta = faltas[idx] || [];
          return (
            <div key={n.fila} className="pedido-nuevo">
              <div className="pedido-nuevo-cabeza">
                <input
                  className={`pedido-campo pedido-nuevo-nombre${falta.includes('Nombre') ? ' pedido-campo-falta' : ''}`}
                  aria-label="Nombre del producto nuevo"
                  value={n.nombre}
                  onChange={(e) => poner(idx, 'nombre', e.target.value)}
                />
                <button type="button" className="pedido-nuevo-quitar" aria-label={`Quitar ${n.nombre}`} onClick={() => onCambio(nuevos.filter((_, i) => i !== idx))}>
                  ✕
                </button>
              </div>
              <p className="pedido-nuevo-datos">
                {repuestos && (
                  <>
                    {[n.marca, n.numero_parte && `N.° parte ${n.numero_parte}`, n.aplicacion].filter(Boolean).join(' · ') || 'Sin marca ni N.° de parte'}
                    {' · '}
                  </>
                )}
                <strong>
                  {n.cantidad} × S/ {soles(n.precio_compra)}
                </strong>
              </p>
              <div className="pedido-nuevo-campos">
                <label>
                  Código
                  <input
                    className={`pedido-campo${falta.some((f) => f.startsWith('Código')) ? ' pedido-campo-falta' : ''}`}
                    value={n.codigo}
                    onChange={(e) => poner(idx, 'codigo', e.target.value)}
                  />
                </label>
                <label>
                  Categoría
                  <select
                    className={`pedido-campo${falta.includes('Categoría') ? ' pedido-campo-falta' : ''}`}
                    value={n.categoria_id}
                    onChange={(e) => poner(idx, 'categoria_id', e.target.value)}
                  >
                    <option value="">Elige…</option>
                    {categorias.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.nombre}
                      </option>
                    ))}
                  </select>
                </label>
                {!(Number(n.precio_compra) > 0) && (
                  <label>
                    Costo c/u
                    <CampoPrecio valor={n.precio_compra} etiqueta={`Costo de ${n.nombre}`} requerido onCambio={(v) => poner(idx, 'precio_compra', v || 0)} />
                  </label>
                )}
                <label>
                  {n.suelto ? `Precio por ${etiquetaUnidad(n.unidad_base).toLowerCase()}` : 'P1'}
                  <CampoPrecio valor={n.precio} etiqueta={`Precio 1 de ${n.nombre}`} requerido onCambio={(v) => poner(idx, 'precio', v)} />
                </label>
                {tresPrecios && (
                  <label>
                    P2
                    <CampoPrecio valor={n.precio_2} etiqueta={`Precio 2 de ${n.nombre}`} onCambio={(v) => poner(idx, 'precio_2', v)} />
                  </label>
                )}
                {tresPrecios && (
                  <label>
                    P3
                    <CampoPrecio valor={n.precio_3} etiqueta={`Precio 3 de ${n.nombre}`} onCambio={(v) => poner(idx, 'precio_3', v)} />
                  </label>
                )}
              </div>
              <label className="pedido-vence">
                <input type="checkbox" checked={!!n.lleva_vencimiento} onChange={(e) => poner(idx, 'lleva_vencimiento', e.target.checked)} />
                Tiene vencimiento (al recibir se pide el lote y la fecha)
              </label>
              {vendeSuelto && (
                <div className={`pedido-suelto${n.suelto ? ' activo' : ''}`}>
                  <label className="pedido-suelto-check">
                    <input type="checkbox" checked={!!n.suelto} onChange={(e) => poner(idx, 'suelto', e.target.checked)} />
                    Llega por caja, pero se vende suelto (por unidad, blíster o caja)
                  </label>
                  {n.suelto && (
                    <div className="pedido-nuevo-campos">
                      <label>
                        Se vende por
                        <select className="pedido-campo" value={n.unidad_base || 'UNIDAD'} onChange={(e) => poner(idx, 'unidad_base', e.target.value)}>
                          {opcionesUnidad(unidadesActivas, n.unidad_base).map((u) => (
                            <option key={u.valor} value={u.valor}>
                              {u.label}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label>
                        La caja trae
                        <input
                          className={`pedido-campo${falta.includes('Cuántas trae la caja') ? ' pedido-campo-falta' : ''}`}
                          type="number"
                          min="2"
                          step="1"
                          value={n.factor ?? ''}
                          onChange={(e) => poner(idx, 'factor', parseInt(e.target.value, 10) || null)}
                        />
                      </label>
                      <label>
                        Precio de la caja
                        <CampoPrecio valor={n.precio_caja} etiqueta={`Precio de la caja de ${n.nombre}`} onCambio={(v) => poner(idx, 'precio_caja', v)} />
                      </label>
                      <label>
                        Blíster de
                        <input
                          className="pedido-campo"
                          type="number"
                          min="2"
                          step="1"
                          placeholder="10"
                          value={n.blister_factor ?? ''}
                          onChange={(e) => poner(idx, 'blister_factor', parseInt(e.target.value, 10) || null)}
                        />
                      </label>
                      <label>
                        Precio blíster
                        <CampoPrecio valor={n.blister_precio} etiqueta={`Precio del blíster de ${n.nombre}`} onCambio={(v) => poner(idx, 'blister_precio', v)} />
                      </label>
                    </div>
                  )}
                  {n.suelto && Number(n.factor) > 1 && (
                    <p className="pedido-suelto-resumen">
                      Entran {Math.round(n.cantidad * n.factor * 1000) / 1000} {plural(etiquetaUnidad(n.unidad_base).toLowerCase())} a S/{' '}
                      {(Number(n.precio_compra) / Number(n.factor)).toFixed(4)} c/u
                      {!(Number(n.precio_caja) > 0) && Number(n.precio) > 0 ? ` · la caja se venderá a S/ ${(Number(n.precio) * Number(n.factor)).toFixed(2)}` : ''}
                    </p>
                  )}
                </div>
              )}
              {falta.length > 0 && <p className="pedido-nuevo-falta">Falta: {falta.join(', ')}</p>}
            </div>
          );
        })}
      </div>
    </div>
  );
}

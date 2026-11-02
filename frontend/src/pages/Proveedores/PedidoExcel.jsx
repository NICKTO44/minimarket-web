import { useRef, useState } from 'react';
import { api } from '../../api/api';
import { ErrorDeArchivo, TIPO_XLSX, escribirLibro, leerHoja } from '../../utils/hojas';
import { descargarArchivo } from '../../utils/pdfTicket';
import { hoyLima } from '../../utils/formato';
import { mapaPorProducto } from '../../utils/repuestos';
import { mapaPrecios } from '../../utils/tresPrecios';
import { FILTROS_PEDIDO, hojaPedido, leerPedido, productosDelPedido } from '../../utils/pedidoProveedor';
import './PedidoExcel.css';

// Módulo "Venta en tabla": pedido a proveedor por Excel (ver utils/pedidoProveedor.js).

// Ficha del repuesto (marca, n.° de parte) y Precio 2 / 3: si el negocio no
// tiene esos módulos, las columnas salen vacías y el pedido funciona igual.
async function datosExtra() {
  const [fichas, precios] = await Promise.all([
    api.repuestosFichas().catch(() => []),
    api.preciosVenta().catch(() => []),
  ]);
  return { fichas: mapaPorProducto(fichas), precios: mapaPrecios(precios) };
}

/** Ventana "Exportar pedido". */
export function ExportarPedido({ productos, categorias, onCerrar }) {
  const [filtro, setFiltro] = useState('BAJO_MINIMO');
  const [categoriaId, setCategoriaId] = useState('');
  const [completo, setCompleto] = useState(false);
  const [ocupado, setOcupado] = useState(false);
  const [error, setError] = useState(null);

  const lista = productosDelPedido(productos, { filtro, categoriaId });

  const descargar = async () => {
    if (lista.length === 0 || ocupado) return;
    setOcupado(true);
    setError(null);
    try {
      const { fichas, precios } = await datosExtra();
      // La cantidad sugerida solo con "bajo stock mínimo"; con todos o una categoría la columna va vacía.
      const hoja = hojaPedido(lista, { completo, fichas, precios, sugerir: filtro === 'BAJO_MINIMO' });
      const nombre = `Pedido-${completo ? 'completo-' : ''}${hoyLima()}.xlsx`;
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
          Descarga un Excel con tus productos y una columna <strong>Cantidad a pedir</strong>. Complétala y súbela en
          <strong> Nueva compra → Cargar desde Excel</strong>.
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

        <fieldset className="pedido-grupo">
          <legend>¿Para quién?</legend>
          <label className="pedido-opcion">
            <input type="radio" name="pedido-tipo" checked={!completo} onChange={() => setCompleto(false)} />
            <span>
              <strong>Para el proveedor</strong> — código, N.° de parte, marca, producto, aplicación y cantidad. Sin tu stock ni tus precios.
            </span>
          </label>
          <label className="pedido-opcion">
            <input type="radio" name="pedido-tipo" checked={completo} onChange={() => setCompleto(true)} />
            <span>
              <strong>Completo (solo para ti)</strong> — además stock, stock mínimo, costo actual y P1 / P2 / P3.
            </span>
          </label>
        </fieldset>

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
 * debajo del botón queda el resumen de lo leído.
 */
export function CargarPedido({ productos, onCargar }) {
  const entrada = useRef(null);
  const [ocupado, setOcupado] = useState(false);
  const [aviso, onAviso] = useState(null);

  const cargar = async (archivo) => {
    if (!archivo) return;
    setOcupado(true);
    onAviso(null);
    try {
      const [filas, { fichas, precios }] = await Promise.all([leerHoja(archivo), datosExtra()]);
      const r = leerPedido(filas, productos, fichas, precios);
      if (r.error) {
        onAviso({ tipo: 'error', texto: r.error });
        return;
      }
      if (r.items.length === 0 && r.precios.length === 0 && r.sinReconocer.length === 0 && r.malas.length === 0) {
        onAviso({ tipo: 'error', texto: 'El Excel no tiene ninguna fila con "Cantidad a pedir".' });
        return;
      }
      onCargar(r.items, r.precios);
      const partes = [`Se cargaron ${r.items.length} producto${r.items.length === 1 ? '' : 's'}. Revisa cantidades y costos antes de guardar.`];
      if (r.precios.length > 0) {
        partes.push(`${r.precios.length} producto${r.precios.length === 1 ? '' : 's'} con precio de venta nuevo (ver abajo).`);
      }
      if (r.sinCosto > 0) partes.push(`${r.sinCosto} sin costo: escríbelo en la lista.`);
      const lista = (xs) => xs.slice(0, 6).map((x) => `fila ${x.fila} (${x.texto})`).join(', ') + (xs.length > 6 ? `… y ${xs.length - 6} más` : '');
      if (r.sinReconocer.length) partes.push(`No se reconocieron: ${lista(r.sinReconocer)}.`);
      if (r.malas.length) partes.push(`Con error: ${lista(r.malas)}.`);
      onAviso({ tipo: r.sinReconocer.length || r.malas.length || r.sinCosto ? 'error' : 'exito', texto: partes.join(' ') });
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
      {aviso && (
        <p className={`pedido-aviso pedido-aviso-${aviso.tipo}`} role="status">
          {aviso.texto}
        </p>
      )}
    </div>
  );
}

const soles = (n) => (n == null ? '—' : Number(n).toFixed(2));

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

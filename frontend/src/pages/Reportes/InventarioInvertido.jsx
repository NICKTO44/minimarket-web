import { Fragment, useEffect, useState } from 'react';
import { api } from '../../api/api';

// Ganancias → "Tu dinero en mercadería": cuánto dinero hay ahora mismo en
// los productos de la tienda (stock × costo), cuánto valdría a precio de
// venta y la diferencia. Solo el administrador, con el módulo Ganancias.
// Ver GET /ganancias/inventario (handlers/ganancias.rs).

const numero = (valor) => valor.toLocaleString('es-PE', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const soles = (valor) => (valor < 0 ? `− S/ ${numero(-valor)}` : `S/ ${numero(valor)}`);
const cantidad = (valor) => valor.toLocaleString('es-PE', { maximumFractionDigits: 3 });
const porcentaje = (parte, total) => (total > 0 ? `${((parte / total) * 100).toLocaleString('es-PE', { maximumFractionDigits: 1 })} %` : '—');

export default function InventarioInvertido({ onVerSinPrecio }) {
  const [datos, setDatos] = useState(null);
  const [error, setError] = useState(null);
  const [abierta, setAbierta] = useState(null);

  useEffect(() => {
    let vivo = true;
    api
      .gananciasInventario()
      .then((d) => vivo && setDatos(d))
      .catch((e) => vivo && setError(e.message));
    return () => {
      vivo = false;
    };
  }, []);

  return (
    <div className="rep-seccion">
      <div className="gan-cabecera">
        <h2>Tu dinero en mercadería</h2>
      </div>
      {error && <p className="gan-neta-nota">No se pudo calcular: {error}</p>}
      {!datos && !error && <p className="gan-neta-nota">Calculando…</p>}
      {datos && (
        <>
          <div className="rep-tarjetas gan-tarjetas">
            <div className="rep-tarjeta rep-tarjeta-destacada">
              <span className="rep-tarjeta-label">Invertido en mercadería</span>
              <strong className="rep-tarjeta-valor">{soles(datos.invertido)}</strong>
              <span className="rep-tarjeta-sub">
                Lo que te costó lo que hay hoy en tu tienda · {cantidad(datos.unidades)} unid. en {datos.productos_con_stock} productos
              </span>
            </div>
            <div className="rep-tarjeta">
              <span className="rep-tarjeta-label">Valor a precio de venta</span>
              <strong className="rep-tarjeta-valor">{soles(datos.valor_venta)}</strong>
              <span className="rep-tarjeta-sub">Lo que recibirías si vendes todo</span>
            </div>
            <div className="rep-tarjeta">
              <span className="rep-tarjeta-label">Ganancia por realizar</span>
              <strong className="rep-tarjeta-valor">{soles(datos.ganancia_por_realizar)}</strong>
              <span className="rep-tarjeta-sub">La diferencia, cuando lo vendas</span>
            </div>
          </div>
          {datos.sin_precio > 0 && (
            <p className="gan-neta-nota">
              {datos.sin_precio} producto{datos.sin_precio === 1 ? '' : 's'} con stock sin precio de compra no se cuenta
              {datos.sin_precio === 1 ? '' : 'n'}.{' '}
              {onVerSinPrecio && (
                <button type="button" className="gan-enlace" onClick={onVerSinPrecio}>
                  Ver cuáles
                </button>
              )}
            </p>
          )}
          {datos.categorias.length > 0 && (
            <div className="gan-tarjeta">
              <table className="rep-tabla">
                <thead>
                  <tr>
                    <th>Categoría</th>
                    <th className="gan-num">Unidades</th>
                    <th className="gan-num">Invertido</th>
                    <th className="gan-num">% del total</th>
                    <th className="gan-num">A precio de venta</th>
                  </tr>
                </thead>
                <tbody>
                  {datos.categorias.map((c) => (
                    <Fragment key={c.categoria}>
                      <tr
                        className="gan-fila-abrible"
                        onClick={() => setAbierta(abierta === c.categoria ? null : c.categoria)}
                        aria-expanded={abierta === c.categoria}
                      >
                        <td>
                          {abierta === c.categoria ? '▾' : '▸'} {c.categoria}
                          <small className="gan-categoria">
                            {c.productos} producto{c.productos === 1 ? '' : 's'}
                          </small>
                        </td>
                        <td className="gan-num">{cantidad(c.unidades)}</td>
                        <td className="gan-num">{soles(c.invertido)}</td>
                        <td className="gan-num">{porcentaje(c.invertido, datos.invertido)}</td>
                        <td className="gan-num">{soles(c.valor_venta)}</td>
                      </tr>
                      {abierta === c.categoria &&
                        datos.productos
                          .filter((p) => p.categoria === c.categoria)
                          .map((p) => (
                            <tr key={p.producto_id} className="gan-fila-detalle">
                              <td>
                                {p.nombre}
                                <small className="gan-categoria">
                                  {cantidad(p.stock)} × {soles(p.costo_unitario)}
                                </small>
                              </td>
                              <td className="gan-num">{cantidad(p.stock)}</td>
                              <td className="gan-num">{soles(p.invertido)}</td>
                              <td className="gan-num">{porcentaje(p.invertido, datos.invertido)}</td>
                              <td className="gan-num">{soles(p.valor_venta)}</td>
                            </tr>
                          ))}
                    </Fragment>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <p className="gan-neta-nota">
            Se calcula ahora mismo con el stock de cada producto por su costo (el promedio de compra o, si no hay, el precio de compra).
            Los productos vencidos y los agotados no se cuentan; los six pack y cajas ya están dentro de sus unidades.
          </p>
        </>
      )}
    </div>
  );
}

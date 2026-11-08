import { useState } from 'react';
import { api } from '../../api/api';
import { ErrorDeArchivo, leerHoja } from '../../utils/hojas';
import './ImportarProductos.css';

// Módulo PRESENTACIONES: cargar six packs, cajas, cajetillas... desde un
// Excel (por ejemplo, el de un sistema anterior). Cada fila es una
// presentación de un producto que YA está en Productos, buscado por su
// código. Si el producto ya tiene una presentación con ese nombre, se
// actualiza; las demás que tenga no se tocan. Nada se guarda sin revisar.

const MAXIMO_FILAS = 2000;

// Títulos que se reconocen solos (sin tildes ni mayúsculas).
const COLUMNAS = {
  codigo_producto: ['codigo de la unidad', 'codigo del producto', 'codigo producto', 'codigo unidad', 'codigo'],
  nombre: ['presentacion', 'nombre de la presentacion', 'nombre'],
  factor: ['factor (descuenta)', 'factor', 'lleva', 'cantidad', 'unidades'],
  precio: ['precio de la presentacion', 'precio presentacion', 'precio'],
  codigo_barras: ['codigo de barras del pack', 'codigo de barras de la presentacion', 'codigo de barras', 'codigo barras'],
};
const sinTildes = (t) =>
  String(t || '')
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .trim();
const numero = (t) => {
  const n = parseFloat(String(t ?? '').trim().replace(',', '.'));
  return Number.isFinite(n) ? n : null;
};

/** Índice de cada campo en la fila de títulos (el primero que coincida). */
function columnasDe(titulos) {
  const t = titulos.map(sinTildes);
  const usadas = new Set();
  const indice = {};
  // Primero los títulos más largos, para que "código de barras" no se tome como "código".
  const pedidos = Object.entries(COLUMNAS).flatMap(([campo, nombres]) => nombres.map((n) => [campo, n]));
  pedidos.sort((a, b) => b[1].length - a[1].length);
  for (const [campo, nombre] of pedidos) {
    if (indice[campo] != null) continue;
    const i = t.findIndex((x, j) => !usadas.has(j) && x === nombre);
    if (i >= 0) {
      indice[campo] = i;
      usadas.add(i);
    }
  }
  return indice;
}

export default function ImportarPresentaciones({ onCerrar, onTerminado }) {
  const [ocupado, setOcupado] = useState(false);
  const [error, setError] = useState(null);
  const [filas, setFilas] = useState(null);
  const [revision, setRevision] = useState(null);
  const [nombreArchivo, setNombreArchivo] = useState('');

  const abrir = async (archivo) => {
    if (!archivo) return;
    setError(null);
    setRevision(null);
    setOcupado(true);
    try {
      const hoja = await leerHoja(archivo);
      const iTitulos = hoja.findIndex((f) => f.some((c) => String(c).trim() !== ''));
      if (iTitulos < 0) throw new ErrorDeArchivo('El archivo está vacío.');
      const col = columnasDe(hoja[iTitulos]);
      const faltan = ['codigo_producto', 'nombre', 'factor', 'precio'].filter((c) => col[c] == null);
      if (faltan.length) {
        throw new ErrorDeArchivo(
          'No encontré las columnas: "Código de la unidad", "Presentación", "Factor" y "Precio" (el "Código de barras" es opcional). Revisa los títulos de la primera fila.'
        );
      }
      const armadas = [];
      hoja.forEach((f, i) => {
        if (i <= iTitulos || !f.some((c) => String(c).trim() !== '')) return;
        armadas.push({
          fila: i + 1,
          codigo_producto: String(f[col.codigo_producto] ?? '').trim(),
          nombre: String(f[col.nombre] ?? '').trim(),
          factor: numero(f[col.factor]),
          precio: numero(f[col.precio]),
          codigo_barras: col.codigo_barras != null ? String(f[col.codigo_barras] ?? '').trim() || null : null,
        });
      });
      if (armadas.length === 0) throw new ErrorDeArchivo('El archivo no tiene filas debajo de los títulos.');
      if (armadas.length > MAXIMO_FILAS) throw new ErrorDeArchivo(`Máximo ${MAXIMO_FILAS} presentaciones por archivo.`);
      setNombreArchivo(archivo.name);
      setFilas(armadas);
      setRevision(await api.presentacionesImportar({ filas: armadas, solo_revisar: true }));
    } catch (e) {
      setFilas(null);
      setError(e.message);
    } finally {
      setOcupado(false);
    }
  };

  const guardar = async () => {
    setOcupado(true);
    setError(null);
    try {
      const r = await api.presentacionesImportar({ filas, solo_revisar: false });
      onTerminado(r);
    } catch (e) {
      setError(e.message);
      setOcupado(false);
    }
  };

  const porFila = new Map(filas?.map((f) => [f.fila, f]) || []);
  const validas = revision ? revision.nuevas + revision.actualizadas : 0;

  return (
    <div className="inv-modal-overlay" onClick={revision ? undefined : onCerrar}>
      <div className="inv-modal imp-modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Cargar presentaciones">
        {!revision ? (
          <>
            <h2>Cargar presentaciones</h2>
            <p className="imp-nota">
              Six pack, caja x12, cajetilla x20… de productos que ya están en Productos. Columnas: <strong>Código de la unidad</strong>,{' '}
              <strong>Presentación</strong>, <strong>Factor</strong> (cuántas unidades descuenta), <strong>Precio</strong> y, si tiene,{' '}
              <strong>Código de barras</strong> del pack. Nada se guarda hasta que revises.
            </p>
            <label className="imp-zona">
              <input
                type="file"
                accept=".xlsx,.csv,.txt"
                onChange={(e) => {
                  abrir(e.target.files?.[0]);
                  e.target.value = '';
                }}
              />
              <strong>{ocupado ? 'Revisando…' : 'Elige tu archivo'}</strong>
              <span>Excel (.xlsx) o CSV</span>
            </label>
            {error && <p className="inv-mensaje inv-mensaje-error">{error}</p>}
            <div className="inv-modal-acciones">
              <button className="inv-boton-cancelar" onClick={onCerrar}>
                Cancelar
              </button>
            </div>
          </>
        ) : (
          <>
            <h2>Revisa antes de cargar</h2>
            <p className="imp-nota">
              {nombreArchivo}: <strong>{revision.nuevas}</strong> nueva{revision.nuevas === 1 ? '' : 's'}
              {revision.actualizadas > 0 && (
                <>
                  , <strong>{revision.actualizadas}</strong> se actualiza{revision.actualizadas === 1 ? '' : 'n'} (ya existía con ese nombre)
                </>
              )}
              {revision.errores > 0 && (
                <>
                  {' '}
                  y <strong>{revision.errores}</strong> con error (no entra{revision.errores === 1 ? '' : 'n'})
                </>
              )}
              .
            </p>
            <div className="imp-tabla-caja imp-tabla-revision">
              <table className="imp-tabla">
                <thead>
                  <tr>
                    <th>Fila</th>
                    <th>Producto</th>
                    <th>Presentación</th>
                    <th>Factor</th>
                    <th>Precio</th>
                    <th>Código de barras</th>
                    <th>Estado</th>
                  </tr>
                </thead>
                <tbody>
                  {revision.filas.map((r) => {
                    const f = porFila.get(r.fila) || {};
                    return (
                      <tr key={r.fila} className={r.estado === 'ERROR' ? 'imp-fila-error' : r.estado === 'ACTUALIZA' ? 'imp-fila-existe' : ''}>
                        <td className="imp-num">{r.fila}</td>
                        <td>{r.producto || f.codigo_producto}</td>
                        <td>{f.nombre}</td>
                        <td className="imp-num">{f.factor ?? '—'}</td>
                        <td className="imp-num">{f.precio != null ? `S/ ${f.precio.toFixed(2)}` : '—'}</td>
                        <td>{f.codigo_barras || '—'}</td>
                        <td className="imp-estado">
                          {r.estado === 'ERROR' ? r.motivo : r.estado === 'ACTUALIZA' ? 'Se actualiza' : 'Nueva'}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            {error && <p className="inv-mensaje inv-mensaje-error">{error}</p>}
            <div className="inv-modal-acciones">
              <button
                className="inv-boton-cancelar"
                onClick={() => {
                  setRevision(null);
                  setFilas(null);
                }}
                disabled={ocupado}
              >
                Elegir otro archivo
              </button>
              <button className="inv-boton-guardar" onClick={guardar} disabled={ocupado || validas === 0}>
                {ocupado ? 'Guardando…' : `Cargar ${validas} presentaci${validas === 1 ? 'ón' : 'ones'}`}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

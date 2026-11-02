import { ANIO_MINIMO } from '../../utils/libros';

// Sección "Ficha del libro" (módulo FICHA_LIBRO) del formulario de producto.
// Todo es opcional: un producto sin nada escrito aquí es un producto común
// (cuadernos, lapiceros...). `editoriales` y `autores`: los ya usados, para
// sugerir al escribir y que no queden "Alfaguara" y "alfaguara " por separado.
export default function FichaLibro({ campos, onCambiar, editoriales = [], autores = [] }) {
  const cambiar = (campo, valor) => onCambiar({ ...campos, [campo]: valor });
  return (
    <div className="inv-ficha-caja">
      <p className="inv-lote-titulo">Ficha del libro</p>
      <p className="inv-lote-nota">
        Opcional. En el punto de venta el libro se encuentra por título, autor, editorial o serie. El ISBN va en el
        campo Código: así se escanea el código de barras del libro.
      </p>
      <div className="inv-ficha-campos">
        <div className="inv-campo inv-ficha-ancho">
          <label>Autor</label>
          <input
            value={campos.autor}
            maxLength={150}
            list="inv-autores-libro"
            placeholder="Mario Vargas Llosa (varios: separa con ;)"
            onChange={(e) => cambiar('autor', e.target.value)}
          />
          <datalist id="inv-autores-libro">
            {autores.map((a) => (
              <option key={a} value={a} />
            ))}
          </datalist>
        </div>
        <div className="inv-campo">
          <label>Editorial</label>
          <input
            value={campos.editorial}
            maxLength={80}
            list="inv-editoriales-libro"
            placeholder="Alfaguara, Santillana, SM…"
            onChange={(e) => cambiar('editorial', e.target.value)}
          />
          <datalist id="inv-editoriales-libro">
            {editoriales.map((ed) => (
              <option key={ed} value={ed} />
            ))}
          </datalist>
        </div>
        <div className="inv-campo">
          <label>Serie o colección</label>
          <input
            value={campos.serie}
            maxLength={80}
            placeholder="Harry Potter, Biblioteca Escolar…"
            onChange={(e) => cambiar('serie', e.target.value)}
          />
        </div>
        <div className="inv-campo">
          <label>Año</label>
          <input
            type="number"
            inputMode="numeric"
            min={ANIO_MINIMO}
            value={campos.anio}
            placeholder="2010"
            onChange={(e) => cambiar('anio', e.target.value)}
          />
        </div>
        <div className="inv-campo">
          <label>Edición</label>
          <input
            value={campos.edicion}
            maxLength={40}
            placeholder="2.ª edición, bolsillo…"
            onChange={(e) => cambiar('edicion', e.target.value)}
          />
        </div>
      </div>
    </div>
  );
}

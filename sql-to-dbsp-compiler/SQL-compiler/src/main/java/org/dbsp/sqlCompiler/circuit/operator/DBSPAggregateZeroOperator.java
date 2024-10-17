package org.dbsp.sqlCompiler.circuit.operator;

import org.dbsp.sqlCompiler.compiler.frontend.TypeCompiler;
import org.dbsp.sqlCompiler.compiler.frontend.calciteObject.CalciteObject;
import org.dbsp.sqlCompiler.compiler.visitors.VisitDecision;
import org.dbsp.sqlCompiler.compiler.visitors.outer.CircuitVisitor;
import org.dbsp.sqlCompiler.ir.NonCoreIR;
import org.dbsp.sqlCompiler.ir.expression.DBSPExpression;

import java.util.List;

/** This operator represents a computation that is used aftr a global
 * aggregation (no group-by) to replace empty results with the expected zero.
 * This operator only used in the front-end, and is later expanded into the 
 * following subgraph:
 * 
 * <p>
 * The input is a zset like {}/{c->1}: either the empty set (for an empty input)
 * or the correct count with a weight of 1.
 * We need to produce {z->1}/{c->1}, where z is the actual zero of the fold above.
 * For this we synthesize the following graph:
 *     |
 * {}/{c->1}------------------------
 *    | map (|x| x -> z}           |
 * {}/{z->1}                       |
 *    | -                          |
 * {} {z->-1}   {z->1} (constant)  |
 *          \  /                  /
 *           +                   /
 *         {z->1}/{}  -----------
 *                 \ /
 *                  +
 *              {z->1}/{c->1}
 *                  |
 */
@NonCoreIR
public class DBSPAggregateZeroOperator extends DBSPUnaryOperator {
    /** Create an AggregateZero operator.
     *
     * @param node   Calcite node.
     * @param zero   Value of zero produced when input is empty.
     * @param source Input from aggregation.
     */
    public DBSPAggregateZeroOperator(
            CalciteObject node, DBSPExpression zero, DBSPOperator source) {
        super(node, "aggregate_zero", zero, TypeCompiler.makeZSet(zero.getType()),
                false, source);
    }

    @Override
    public DBSPOperator withInputs(List<DBSPOperator> newInputs, boolean force) {
        if (force || this.inputsDiffer(newInputs))
            return new DBSPAggregateZeroOperator(this.getNode(), this.getFunction(),
                    newInputs.get(0))
                    .copyAnnotations(this);
        return this;
    }

    @Override
    public void accept(CircuitVisitor visitor) {
        visitor.push(this);
        VisitDecision decision = visitor.preorder(this);
        if (!decision.stop())
            visitor.postorder(this);
        visitor.pop(this);
    }

    @Override
    public boolean equivalent(DBSPOperator other) {
        if (!super.equivalent(other))
            return false;
        DBSPAggregateZeroOperator otherOperator = other.as(DBSPAggregateZeroOperator.class);
        if (otherOperator == null)
            return false;
        return this.function.equivalent(otherOperator.function);
    }
}

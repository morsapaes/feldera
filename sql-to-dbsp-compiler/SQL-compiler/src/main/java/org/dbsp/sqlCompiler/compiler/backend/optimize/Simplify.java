/*
 * Copyright 2023 VMware, Inc.
 * SPDX-License-Identifier: MIT
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

package org.dbsp.sqlCompiler.compiler.backend.optimize;

import org.dbsp.sqlCompiler.compiler.backend.DBSPCompiler;
import org.dbsp.sqlCompiler.compiler.backend.visitors.InnerRewriteVisitor;
import org.dbsp.sqlCompiler.ir.expression.DBSPBinaryExpression;
import org.dbsp.sqlCompiler.ir.expression.DBSPCastExpression;
import org.dbsp.sqlCompiler.ir.expression.DBSPExpression;
import org.dbsp.sqlCompiler.ir.expression.DBSPIfExpression;
import org.dbsp.sqlCompiler.ir.expression.DBSPOpcode;
import org.dbsp.sqlCompiler.ir.expression.literal.DBSPBoolLiteral;
import org.dbsp.sqlCompiler.ir.expression.DBSPIsNullExpression;
import org.dbsp.sqlCompiler.ir.expression.literal.DBSPLiteral;
import org.dbsp.sqlCompiler.ir.type.primitive.DBSPTypeNull;

import java.util.Objects;

/**
 * Visitor which does some Rust-level expression simplifications.
 * - is_null() called on non-nullable values is simplified to 'false'
 * - Boolean && and || with constant arguments are simplified
 * - 'if' expressions with constant arguments are simplified to the corresponding branch
 * - cast(NULL, T) is converted to a NULL value of type T
 */
public class Simplify extends InnerRewriteVisitor {
    // You would think that Calcite has done these optimizations, but apparently not.

    public Simplify(DBSPCompiler compiler) {
        super(compiler);
    }

    @Override
    public boolean preorder(DBSPIsNullExpression expression) {
        DBSPExpression source = this.transform(expression.expression);
        DBSPExpression result = expression;
        if (!source.getNonVoidType().mayBeNull)
            result = DBSPBoolLiteral.FALSE;
        this.map(expression, result);
        return false;
    }

    @Override
    public boolean preorder(DBSPCastExpression expression) {
        DBSPExpression source = this.transform(expression.source);
        DBSPLiteral lit = source.as(DBSPLiteral.class);
        if (lit != null) {
            if (lit.getNonVoidType().is(DBSPTypeNull.class)) {
                // This is a literal with type "NULL".
                // Convert it to a literal of the resulting type
                DBSPExpression result = DBSPLiteral.none(expression.getNonVoidType());
                this.map(expression, result);
                return false;
            }
        }
        this.map(expression, expression);
        return false;
    }

    @Override
    public boolean preorder(DBSPIfExpression expression) {
        DBSPExpression condition = this.transform(expression.condition);
        DBSPExpression negative = this.transform(expression.negative);
        DBSPExpression positive = this.transform(expression.positive);
        DBSPExpression result = expression;
        if (condition.is(DBSPBoolLiteral.class)) {
            DBSPBoolLiteral cond = condition.to(DBSPBoolLiteral.class);
            if (Objects.requireNonNull(cond.value)) {
                result = positive;
            } else {
                result = negative;
            }
        } else if (condition != expression.condition ||
                positive != expression.positive ||
                negative != expression.negative) {
            result = new DBSPIfExpression(expression.getNode(), condition, positive, negative);
        }
        this.map(expression, result);
        return false;
    }

    @Override
    public boolean preorder(DBSPBinaryExpression expression) {
        DBSPExpression left = this.transform(expression.left);
        DBSPExpression right = this.transform(expression.right);
        DBSPExpression result = expression;
        if (expression.operation.equals(DBSPOpcode.AND)) {
            if (left.is(DBSPBoolLiteral.class)) {
                DBSPBoolLiteral bLeft = left.to(DBSPBoolLiteral.class);
                if (bLeft.isNull) {
                    result = bLeft;
                } else if (bLeft.getNonNullValue(Boolean.class)) {
                    result = right;
                } else {
                    result = left;
                }
            } else if (right.is(DBSPBoolLiteral.class)) {
                DBSPBoolLiteral bRight = right.to(DBSPBoolLiteral.class);
                if (bRight.isNull) {
                    result = left;
                } else if (bRight.getNonNullValue(Boolean.class)) {
                    result = left;
                } else {
                    result = right;
                }
            }
        } else if (expression.operation.equals(DBSPOpcode.OR)) {
            if (left.is(DBSPBoolLiteral.class)) {
                DBSPBoolLiteral bLeft = left.to(DBSPBoolLiteral.class);
                if (bLeft.isNull) {
                    result = bLeft;
                } else if (bLeft.getNonNullValue(Boolean.class)) {
                    result = left;
                } else {
                    result = right;
                }
            } else if (right.is(DBSPBoolLiteral.class)) {
                DBSPBoolLiteral bRight = right.to(DBSPBoolLiteral.class);
                if (bRight.isNull) {
                    result = left;
                } else if (bRight.getNonNullValue(Boolean.class)) {
                    result = right;
                } else {
                    result = left;
                }
            }
        } else if (left != expression.left || right != expression.right) {
            result = new DBSPBinaryExpression(expression.getNode(), expression.getNonVoidType(),
                    expression.operation, left, right, expression.primitive);
        }
        this.map(expression, result);
        return false;
    }
}